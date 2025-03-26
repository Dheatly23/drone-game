mod block_entity;
mod chunk;

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::hash_map::{Entry, HashMap};
use std::future::Future;
use std::mem::{MaybeUninit, replace};
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as FutContext, Poll, Waker};

use boa_engine::job::NativeJob;
use boa_engine::object::builtins::{JsArray, JsArrayBuffer, JsFunction, JsMap, JsPromise, JsSet};
use boa_engine::object::{IntegrityLevel, ObjectInitializer};
use boa_engine::prelude::*;
use boa_engine::property::{Attribute, PropertyKey};
use boa_engine::{JsArgs as _, JsResult, js_error, js_string};
use enumflags2::{BitFlag, BitFlags, make_bitflags};
use uuid::{Bytes, Uuid};

use level_state::{
    ArchivedLevelState, Block, Command, Direction, DroneCapabilityFlags, ExecutionContext,
    InventoryOp, InventorySlot, InventoryType, Item,
};
use util_wasm::ChannelId;

use self::block_entity::{
    CAP_FLAGS_LIST, Wrapper as BlockEntityWrapper, WrapperData as BlockEntityWrapperData,
};
use self::chunk::Chunk;
use crate::UUID;
use crate::util::{format_uuid, js_str_small};

type WakerCell = Rc<Cell<Option<Waker>>>;

pub static mut LEVEL: (u64, Option<&'static ArchivedLevelState>) = (0, None);
pub static mut WAKERS: Vec<Rc<Cell<Option<Waker>>>> = Vec::new();
pub static mut COMMAND: Option<Command> = None;

unsafe fn get_level<'a>() -> JsResult<(u64, &'a ArchivedLevelState)> {
    match unsafe { *(&raw const LEVEL) } {
        (e, Some(l)) => Ok((e, l)),
        (_, None) => Err(JsNativeError::error()
            .with_message("level is not yet initialized.")
            .into()),
    }
}

unsafe fn check_level<'a, T>(
    epoch: &mut u64,
    value: &'a mut T,
    f: impl for<'t> FnOnce(&'t ArchivedLevelState, &'t mut T) -> JsResult<()>,
) -> JsResult<&'a mut T> {
    let (e, l) = unsafe { get_level()? };
    if e != *epoch {
        *epoch = e;
        f(l, value)?;
    }
    Ok(value)
}

fn block_to_str(b: Block) -> JsString {
    match b {
        Block::Air => js_string!("air"),
        Block::Dirt => js_string!("dirt"),
        Block::Grass => js_string!("grass"),
        Block::IronOre => js_string!("iron_ore"),
        _ => js_string!("unknown"),
    }
}

fn item_to_str(i: Item) -> JsString {
    match i {
        Item::Air => js_string!("air"),
        Item::Dirt => js_string!("dirt"),
        Item::Grass => js_string!("grass"),
        Item::IronOre => js_string!("ironOre"),
        Item::Unknown => js_string!("unknown"),
    }
}

#[derive(Debug, Trace, Finalize)]
struct SubscriberCb {
    func: Option<JsFunction>,
    #[unsafe_ignore_trace]
    channel: ChannelId,
}

type BlockEntityCacheType = HashMap<Bytes, JsObject<BlockEntityWrapperData>>;
type BlockEntityCoordsType = Vec<(usize, usize, usize, Uuid)>;

#[derive(Debug, Trace, JsData, Finalize)]
pub struct Level {
    chunk_proto: JsObject,
    chunk_cache: HashMap<[usize; 3], JsObject<Chunk>>,
    block_entity_proto: BlockEntityWrapper,
    block_entity_cache_epoch: u64,
    block_entity_cache: BlockEntityCacheType,
    block_entity_coords_epoch: u64,
    #[unsafe_ignore_trace]
    block_entity_coords: BlockEntityCoordsType,

    #[unsafe_ignore_trace]
    subscribers: HashMap<ChannelId, Vec<i32>>,
    subscriber_callbacks: HashMap<i32, SubscriberCb>,
    subscriber_empty: i32,

    #[unsafe_ignore_trace]
    temp_buf: Rc<RefCell<Vec<MaybeUninit<u8>>>>,
}

impl Level {
    pub fn new_object(ctx: &mut Context) -> JsObject {
        let mut builder = ObjectInitializer::with_native_data(
            Self {
                chunk_proto: Chunk::new_proto(ctx),
                chunk_cache: HashMap::new(),
                block_entity_proto: BlockEntityWrapper::new_proto(ctx),
                block_entity_cache_epoch: 0,
                block_entity_cache: BlockEntityCacheType::new(),
                block_entity_coords_epoch: 0,
                block_entity_coords: BlockEntityCoordsType::new(),

                subscribers: HashMap::new(),
                subscriber_callbacks: HashMap::new(),
                subscriber_empty: 0,

                temp_buf: Rc::default(),
            },
            ctx,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::get_chunk),
            js_string!("getChunk"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::get_block_entity),
            js_string!("getBlockEntity"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::get_block_entity_uuids),
            js_string!("getBlockEntityUuids"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::get_block_entity_uuids_at),
            js_string!("getBlockEntityUuidsAt"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::register_channel),
            js_string!("registerChannel"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::unregister_channel),
            js_string!("unregisterChannel"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::publish),
            js_string!("publishChannel"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::process_subscription),
            js_string!("processSubscription"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::submit),
            js_string!("submit"),
            0,
        );
        builder.function(
            NativeFunction::from_copy_closure(Self::tick),
            js_string!("tick"),
            0,
        );
        let uuid = unsafe { &*(&raw const UUID) };
        builder.property(
            js_string!("uuid"),
            format_uuid(uuid),
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE | Attribute::READONLY,
        );

        let getter = NativeFunction::from_copy_closure(Self::is_initialized)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("initialized"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE | Attribute::READONLY,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_x)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("x"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE | Attribute::READONLY,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_y)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("y"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE | Attribute::READONLY,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_z)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("z"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE | Attribute::READONLY,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_chunk_proto)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("Chunk"),
            Some(getter),
            None,
            Attribute::ENUMERABLE | Attribute::READONLY,
        );
        builder.build()
    }

    fn is_initialized(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        unsafe { Ok((*(&raw const LEVEL)).1.is_some().into()) }
    }

    fn get_x(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        let level = unsafe { get_level()?.1 };
        Ok(level.chunk_size().0.into())
    }

    fn get_y(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        let level = unsafe { get_level()?.1 };
        Ok(level.chunk_size().1.into())
    }

    fn get_z(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        let level = unsafe { get_level()?.1 };
        Ok(level.chunk_size().2.into())
    }

    fn get_chunk_proto(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(Self::downcast_this(this)?
            .borrow()
            .data()
            .chunk_proto
            .clone()
            .into())
    }

    fn get_chunk(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let x = args.get_or_undefined(0).try_js_into::<usize>(ctx)?;
        let y = args.get_or_undefined(1).try_js_into::<usize>(ctx)?;
        let z = args.get_or_undefined(2).try_js_into::<usize>(ctx)?;

        let level = unsafe { get_level()?.1 };

        let (sx, sy, sz) = level.chunk_size();
        if x >= sx {
            return Err(JsNativeError::typ()
                .with_message(format!("index x out of bounds ({x} >= {sx})"))
                .into());
        } else if y >= sy {
            return Err(JsNativeError::typ()
                .with_message(format!("index y out of bounds ({y} >= {sy})"))
                .into());
        } else if z >= sz {
            return Err(JsNativeError::typ()
                .with_message(format!("index z out of bounds ({z} >= {sz})"))
                .into());
        }

        let mut guard = this.borrow_mut();
        let data = guard.data_mut();
        Ok(match data.chunk_cache.entry([x, y, z]) {
            Entry::Occupied(e) => e.get().clone().upcast(),
            Entry::Vacant(e) => {
                let &[x, y, z] = e.key();
                let c = JsObject::new(
                    ctx.root_shape(),
                    Some(data.chunk_proto.clone()),
                    Chunk { x, y, z },
                );
                let c = e.insert(c).clone().upcast();
                c.set_integrity_level(IntegrityLevel::Frozen, ctx)?;
                c
            }
        }
        .into())
    }

    fn get_block_entity_coords(&mut self) -> JsResult<&mut BlockEntityCoordsType> {
        unsafe {
            check_level(
                &mut self.block_entity_coords_epoch,
                &mut self.block_entity_coords,
                |level, v| {
                    v.clear();
                    v.extend(level.block_entities().entries().map(|(&k, v)| {
                        (
                            v.x.to_native() as _,
                            v.y.to_native() as _,
                            v.z.to_native() as _,
                            k,
                        )
                    }));
                    v.sort_unstable();
                    Ok(())
                },
            )
        }
    }

    fn get_block_entity_uuids(
        this: &JsValue,
        _: &[JsValue],
        ctx: &mut Context,
    ) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;

        let mut guard = this.borrow_mut();
        let coords = guard.data_mut().get_block_entity_coords()?;

        Ok(JsArray::from_iter(coords.iter().map(|(_, _, _, k)| format_uuid(k).into()), ctx).into())
    }

    fn get_block_entity_uuids_at(
        this: &JsValue,
        args: &[JsValue],
        ctx: &mut Context,
    ) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let x = args.get_or_undefined(0).try_js_into::<usize>(ctx)?;
        let y = args.get_or_undefined(1).try_js_into::<usize>(ctx)?;
        let z = args.get_or_undefined(2).try_js_into::<usize>(ctx)?;

        let mut guard = this.borrow_mut();
        let coords = guard.data_mut().get_block_entity_coords()?;

        let i = coords.partition_point(|&(x_, y_, z_, _)| {
            matches!(
                x_.cmp(&x).then_with(|| y_.cmp(&y)).then_with(|| z_.cmp(&z)),
                Ordering::Less,
            )
        });

        Ok(JsArray::from_iter(
            coords[i..]
                .iter()
                .take_while(|&&(x_, y_, z_, _)| x_ == x && y_ == y && z_ == z)
                .map(|(_, _, _, k)| format_uuid(k).into()),
            ctx,
        )
        .into())
    }

    fn get_block_entity(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let uuid = Uuid::try_parse(
            &args
                .get_or_undefined(0)
                .try_js_into::<JsString>(ctx)?
                .to_std_string()
                .map_err(JsError::from_rust)?,
        )
        .map_err(JsError::from_rust)?;

        let mut guard = this.borrow_mut();
        let data = guard.data_mut();
        let cache = unsafe {
            check_level(
                &mut data.block_entity_cache_epoch,
                &mut data.block_entity_cache,
                |level, cache| {
                    cache.retain(|k, _| {
                        level
                            .block_entities()
                            .get(Uuid::from_bytes_ref(k))
                            .is_some()
                    });
                    Ok(())
                },
            )?
        };
        let slot = match cache.entry(uuid.into_bytes()) {
            Entry::Occupied(v) => return Ok(v.get().clone().upcast().into()),
            Entry::Vacant(v) => v,
        };

        let level = unsafe { get_level()?.1 };
        let Some(be) = level.block_entities().get(Uuid::from_bytes_ref(slot.key())) else {
            return Ok(JsValue::null());
        };
        let obj = data
            .block_entity_proto
            .build(Uuid::from_bytes(*slot.key()), be, ctx)?;
        Ok(slot.insert(obj).clone().upcast().into())
    }

    fn to_vec_u8(value: &JsValue) -> JsResult<Vec<u8>> {
        match value {
            JsValue::Object(o) => Ok(match JsArrayBuffer::from_object(o.clone())?.data() {
                Some(v) => Vec::from(&*v),
                None => Vec::new(),
            }),
            JsValue::String(s) => Ok(s.to_std_string_lossy().into()),
            _ => Err(JsNativeError::typ()
                .with_message("cannot represent argument as Vec<u8>")
                .into()),
        }
    }

    fn register_channel(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let key = Self::to_vec_u8(args.get_or_undefined(0))?;
        let flags = args
            .get_or_undefined(1)
            .try_js_into::<Option<JsObject>>(ctx)?;

        let channel = match flags {
            Some(f) => ChannelId::create(
                &key,
                f.get(js_str!("publish"), ctx)?
                    .try_js_into::<Option<bool>>(ctx)?
                    .unwrap_or_default(),
                f.get(js_str!("subscribe"), ctx)?
                    .try_js_into::<Option<bool>>(ctx)?
                    .unwrap_or_default(),
            ),
            None => ChannelId::create(&key, false, false),
        };
        drop(key);

        {
            let mut this = this.borrow_mut();
            let this = &mut *this.data_mut();

            let entry = this.subscribers.entry(channel.clone());
            entry.key().merge(&channel);

            let ids = entry.or_default();
            let func = if channel.is_subscribe() {
                Some(args.get_or_undefined(2).try_js_into::<JsFunction>(ctx)?)
            } else {
                None
            };

            for _ in 0..u32::MAX {
                let k = this.subscriber_empty;
                this.subscriber_empty = this.subscriber_empty.wrapping_add(1);
                let Entry::Vacant(e) = this.subscriber_callbacks.entry(k) else {
                    continue;
                };
                e.insert(SubscriberCb { func, channel });
                ids.push(k);
                return Ok(k.into());
            }
        }

        Err(JsNativeError::error()
            .with_message("cannot register function, index is full")
            .into())
    }

    fn unregister_channel(
        this: &JsValue,
        args: &[JsValue],
        ctx: &mut Context,
    ) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let key = args.get_or_undefined(0).try_js_into::<i32>(ctx)?;

        let mut this = this.borrow_mut();
        let this = &mut *this.data_mut();
        let Some(v) = this
            .subscriber_callbacks
            .remove_entry(&key)
            .and_then(|(_, v)| {
                v.func.as_ref()?;
                this.subscribers.get_mut(&v.channel)
            })
        else {
            return Err(JsNativeError::error()
                .with_message("channel does not exist")
                .into());
        };
        if let Some(i) = v
            .iter()
            .enumerate()
            .find_map(|(i, &v)| if v == key { Some(i) } else { None })
        {
            v.swap_remove(i);
        }

        Ok(JsValue::undefined())
    }

    fn publish(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let key = args.get_or_undefined(0).try_js_into::<i32>(ctx)?;
        let msg = Self::to_vec_u8(args.get_or_undefined(1))?;

        {
            let mut this = this.borrow_mut();
            let this = &mut *this.data_mut();

            let Some((c, _)) = this
                .subscriber_callbacks
                .get(&key)
                .and_then(|k| this.subscribers.get_key_value(&k.channel))
            else {
                return Err(JsNativeError::error()
                    .with_message("channel does not exist")
                    .into());
            };
            if !c.is_publish() {
                return Err(JsNativeError::typ()
                    .with_message("channel is not publishable")
                    .into());
            }
            c.publish(&msg);
        }

        Ok(JsValue::undefined())
    }

    fn process_subscription(this: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let this = this.borrow();
        let this = this.data();

        for (k, v) in &this.subscribers {
            if !k.is_subscribe() || !k.has_message() {
                continue;
            }

            let channel = k.clone();
            let funcs = v
                .iter()
                .filter_map(|k| this.subscriber_callbacks.get(k)?.func.clone())
                .collect::<Vec<_>>();
            let temp = this.temp_buf.clone();
            ctx.enqueue_job(NativeJob::new(move |ctx| {
                let mut errors = Vec::new();

                'main: loop {
                    let mut guard = temp.borrow_mut();
                    let data = loop {
                        match channel.pop_message(&mut guard[..]) {
                            Ok(Some(v)) => break v,
                            Ok(None) => break 'main,
                            Err(n) => guard.resize_with(n, MaybeUninit::uninit),
                        }
                    };
                    let data: JsValue = match String::from_utf8(data.to_owned()) {
                        Ok(v) => JsString::from(&*v).into(),
                        Err(e) => JsArrayBuffer::from_byte_block(e.into_bytes(), ctx)?.into(),
                    };
                    drop(guard);

                    let this = JsValue::from(ctx.global_object());
                    let args = [data];
                    for f in &funcs {
                        if let Err(e) = f.call(&this, &args, ctx) {
                            errors.push(e);
                        }
                    }
                }

                if !errors.is_empty() {
                    return Err(JsNativeError::aggregate(errors).into());
                }
                Ok(JsValue::undefined())
            }));
        }

        Ok(JsValue::undefined())
    }

    fn submit(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let cmd = args.get_or_undefined(0).try_js_into::<JsObject>(ctx)?;

        fn get_dir(obj: &JsObject, ctx: &mut Context) -> JsResult<Option<Direction>> {
            let r = match js_str_small(
                obj.get(js_string!("direction"), ctx)?
                    .try_js_into::<JsString>(ctx)?
                    .as_str(),
            )
            .as_deref()
            {
                Some("up") => Direction::Up,
                Some("down") => Direction::Down,
                Some("left") => Direction::Left,
                Some("right") => Direction::Right,
                Some("forward") => Direction::Forward,
                Some("backward") => Direction::Back,
                _ => return Ok(None),
            };
            Ok(Some(r))
        }

        fn inv_type_match<S: Deref<Target = str>>(s: Option<S>) -> Option<InventoryType> {
            Some(match s.as_deref()? {
                "main" => InventoryType::Inventory,
                "extended" => InventoryType::ExtInventory,
                _ => return None,
            })
        }

        fn get_inv_slot(obj: &JsObject, ctx: &mut Context) -> JsResult<Option<InventorySlot>> {
            Ok(
                match inv_type_match(js_str_small(
                    obj.get(js_string!("inventory"), ctx)?
                        .try_js_into::<JsString>(ctx)?
                        .as_str(),
                )) {
                    Some(inv) => Some(InventorySlot {
                        inventory: inv,
                        slot: obj.get(js_string!("slot"), ctx)?.try_js_into(ctx)?,
                    }),
                    None => None,
                },
            )
        }

        fn get_inv_op(obj: JsObject, ctx: &mut Context) -> JsResult<Option<InventoryOp>> {
            let Some(s) = js_str_small(
                obj.get(js_string!("operation"), ctx)?
                    .try_js_into::<JsString>(ctx)?
                    .as_str(),
            ) else {
                return Ok(None);
            };

            Ok(match &*s {
                "swap" => {
                    let Some(src) =
                        get_inv_slot(&obj.get(js_string!("source"), ctx)?.try_js_into(ctx)?, ctx)?
                    else {
                        return Ok(None);
                    };
                    let Some(dst) = get_inv_slot(
                        &obj.get(js_string!("destination"), ctx)?.try_js_into(ctx)?,
                        ctx,
                    )?
                    else {
                        return Ok(None);
                    };

                    Some(InventoryOp::Swap { src, dst })
                }
                "transfer" => {
                    let Some(src) =
                        get_inv_slot(&obj.get(js_string!("source"), ctx)?.try_js_into(ctx)?, ctx)?
                    else {
                        return Ok(None);
                    };
                    let Some(dst) = get_inv_slot(
                        &obj.get(js_string!("destination"), ctx)?.try_js_into(ctx)?,
                        ctx,
                    )?
                    else {
                        return Ok(None);
                    };

                    Some(InventoryOp::Transfer {
                        src,
                        dst,
                        count: obj.get(js_string!("count"), ctx)?.try_js_into(ctx)?,
                    })
                }
                "pull" => {
                    let Some(src) = inv_type_match(js_str_small(
                        obj.get(js_string!("source"), ctx)?
                            .try_js_into::<JsString>(ctx)?
                            .as_str(),
                    )) else {
                        return Ok(None);
                    };
                    let Some(dst) = get_inv_slot(
                        &obj.get(js_string!("destination"), ctx)?.try_js_into(ctx)?,
                        ctx,
                    )?
                    else {
                        return Ok(None);
                    };

                    Some(InventoryOp::Pull {
                        src,
                        dst,
                        count: obj.get(js_string!("count"), ctx)?.try_js_into(ctx)?,
                    })
                }
                "push" => {
                    let Some(src) =
                        get_inv_slot(&obj.get(js_string!("source"), ctx)?.try_js_into(ctx)?, ctx)?
                    else {
                        return Ok(None);
                    };
                    let Some(dst) = inv_type_match(js_str_small(
                        obj.get(js_string!("destination"), ctx)?
                            .try_js_into::<JsString>(ctx)?
                            .as_str(),
                    )) else {
                        return Ok(None);
                    };

                    Some(InventoryOp::Push {
                        src,
                        dst,
                        count: obj.get(js_string!("count"), ctx)?.try_js_into(ctx)?,
                    })
                }
                _ => None,
            })
        }

        fn into_cap(obj: &JsObject, ctx: &mut Context) -> JsResult<BitFlags<DroneCapabilityFlags>> {
            let mut r = DroneCapabilityFlags::empty();
            if let Ok(obj) = JsArray::from_object(obj.clone()) {
                for i in 0usize..obj.length(ctx)?.try_into().map_err(JsError::from_rust)? {
                    r |= match js_str_small(obj.get(i, ctx)?.try_js_into::<JsString>(ctx)?.as_str())
                        .as_deref()
                    {
                        Some("move") => make_bitflags!(DroneCapabilityFlags::Moving),
                        Some("fly") => make_bitflags!(DroneCapabilityFlags::Flying),
                        Some("break") => make_bitflags!(DroneCapabilityFlags::Breaker),
                        Some("silkTouch") => make_bitflags!(DroneCapabilityFlags::SilkTouch),
                        Some("backpack") => make_bitflags!(DroneCapabilityFlags::ExtendedInventory),
                        Some("spawn") => make_bitflags!(DroneCapabilityFlags::DroneSummon),
                        _ => continue,
                    };
                }
            } else if let Ok(obj) = JsSet::from_object(obj.clone()) {
                for &(k, v) in CAP_FLAGS_LIST {
                    if obj.has(k, ctx)? {
                        r |= v;
                    }
                }
            } else if let Ok(obj) = JsMap::from_object(obj.clone()) {
                for &(k, v) in CAP_FLAGS_LIST {
                    if obj.get(k, ctx)?.to_boolean() {
                        r |= v;
                    }
                }
            } else {
                for &(k, v) in CAP_FLAGS_LIST {
                    if obj.get(k, ctx)?.to_boolean() {
                        r |= v;
                    }
                }
            }

            Ok(r)
        }

        fn to_cmd(obj: JsObject, ctx: &mut Context) -> JsResult<Option<Command>> {
            let Some(s) = js_str_small(
                obj.get(js_string!("command"), ctx)?
                    .try_js_into::<JsString>(ctx)?
                    .as_str(),
            ) else {
                return Ok(None);
            };

            Ok(match &*s {
                "noop" => Some(Command::Noop),
                "move" => get_dir(&obj, ctx)?.map(Command::Move),
                "place" => {
                    let Some(dir) = get_dir(&obj, ctx)? else {
                        return Ok(None);
                    };
                    let Some(slot) = get_inv_slot(&obj, ctx)? else {
                        return Ok(None);
                    };

                    Some(Command::Place(slot, dir))
                }
                "break" => get_dir(&obj, ctx)?.map(Command::Break),
                "mine" => get_dir(&obj, ctx)?.map(Command::Mine),
                "pullInventory" => {
                    let Some(dir) = get_dir(&obj, ctx)? else {
                        return Ok(None);
                    };
                    let Some(InventorySlot {
                        inventory: src_inv,
                        slot: src_slot,
                    }) = get_inv_slot(&obj.get(js_string!("source"), ctx)?.try_js_into(ctx)?, ctx)?
                    else {
                        return Ok(None);
                    };
                    let Some(InventorySlot {
                        inventory: dst_inv,
                        slot: dst_slot,
                    }) = get_inv_slot(
                        &obj.get(js_string!("destination"), ctx)?.try_js_into(ctx)?,
                        ctx,
                    )?
                    else {
                        return Ok(None);
                    };

                    Some(Command::PullInventory {
                        direction: dir,
                        src_inv,
                        src_slot,
                        dst_inv,
                        dst_slot,
                        count: obj.get(js_string!("count"), ctx)?.try_js_into(ctx)?,
                    })
                }
                "pushInventory" => {
                    let Some(dir) = get_dir(&obj, ctx)? else {
                        return Ok(None);
                    };
                    let Some(InventorySlot {
                        inventory: src_inv,
                        slot: src_slot,
                    }) = get_inv_slot(&obj.get(js_string!("source"), ctx)?.try_js_into(ctx)?, ctx)?
                    else {
                        return Ok(None);
                    };
                    let Some(InventorySlot {
                        inventory: dst_inv,
                        slot: dst_slot,
                    }) = get_inv_slot(
                        &obj.get(js_string!("destination"), ctx)?.try_js_into(ctx)?,
                        ctx,
                    )?
                    else {
                        return Ok(None);
                    };

                    Some(Command::PushInventory {
                        direction: dir,
                        src_inv,
                        src_slot,
                        dst_inv,
                        dst_slot,
                        count: obj.get(js_string!("count"), ctx)?.try_js_into(ctx)?,
                    })
                }
                "inventory" => {
                    let Some(mut ops) = obj
                        .get(js_string!("operations"), ctx)?
                        .try_js_into::<Vec<JsObject>>(ctx)?
                        .into_iter()
                        .map(|o| get_inv_op(o, ctx))
                        .collect::<JsResult<Option<Vec<_>>>>()?
                    else {
                        return Ok(None);
                    };
                    ops.shrink_to_fit();

                    Some(Command::InventoryOps(ops))
                }
                "summon" => {
                    let Some(dir) = get_dir(&obj, ctx)? else {
                        return Ok(None);
                    };

                    let exec: String = obj.get(js_string!("executable"), ctx)?.try_js_into(ctx)?;
                    let args: Vec<String> =
                        obj.get(js_string!("arguments"), ctx)?.try_js_into(ctx)?;
                    let env: JsObject =
                        obj.get(js_string!("environment"), ctx)?.try_js_into(ctx)?;
                    let env = if let Ok(env) = JsMap::from_object(env.clone()) {
                        let mut r = Vec::with_capacity(env.get_size(ctx)?.try_js_into(ctx)?);
                        env.for_each_native(|k, v| {
                            if !matches!((&k, &v), (JsValue::String(_), JsValue::String(_))) {
                                return Ok(());
                            };

                            r.push((k.try_js_into::<String>(ctx)?, v.try_js_into::<String>(ctx)?));
                            Ok(())
                        })?;
                        r
                    } else {
                        env.own_property_keys(ctx)?
                            .into_iter()
                            .filter_map(|k| {
                                let PropertyKey::String(k) = k else {
                                    return None;
                                };
                                let key = match JsValue::from(k.clone()).try_js_into::<String>(ctx)
                                {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(e)),
                                };

                                let r = env
                                    .get(k, ctx)
                                    .and_then(|v| Ok((key, v.try_js_into::<String>(ctx)?)));
                                Some(r)
                            })
                            .collect::<JsResult<Vec<_>>>()?
                    };
                    let exec = ExecutionContext::default()
                        .executable(exec)
                        .args(args)
                        .env(env);

                    let cap = into_cap(
                        &obj.get(js_string!("capability"), ctx)?.try_js_into(ctx)?,
                        ctx,
                    )?;

                    Some(Command::Summon(dir, exec, cap))
                }
                _ => None,
            })
        }

        let cmd = match to_cmd(cmd, ctx) {
            Ok(Some(v)) => v,
            Ok(None) => return Err(js_error!("invalid command")),
            Err(e) => {
                return Err(JsNativeError::error()
                    .with_message("invalid command")
                    .with_cause(e)
                    .into());
            }
        };

        Ok(JsPromise::from_future(CommandFuture::new(unsafe { write_cmd(cmd).err() }), ctx).into())
    }

    fn tick(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        Ok(JsPromise::from_future(CommandFuture::new(None), ctx).into())
    }

    fn downcast_this(this: &JsValue) -> JsResult<JsObject<Self>> {
        if let Some(obj) = this.as_object() {
            if let Ok(ret) = obj.clone().downcast::<Self>() {
                return Ok(ret);
            }
        }

        Err(JsNativeError::typ()
            .with_message("invalid this object type")
            .into())
    }
}

unsafe fn write_cmd(cmd: Command) -> Result<(), Command> {
    let target = unsafe { &mut *(&raw mut COMMAND) };
    if target.is_some() {
        return Err(cmd);
    }

    *target = Some(cmd);
    Ok(())
}

struct CommandFuture {
    cmd: Option<Command>,
    waker: WakerCell,
    first: bool,
}

impl CommandFuture {
    fn new(cmd: Option<Command>) -> Self {
        Self {
            cmd,
            waker: WakerCell::default(),
            first: true,
        }
    }
}

impl Future for CommandFuture {
    type Output = JsResult<JsValue>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut FutContext<'_>) -> Poll<Self::Output> {
        let Self { cmd, waker, first } = &mut *self;

        let first = replace(first, false);
        let waited = waker.replace(Some(cx.waker().clone())).is_none();
        if waited {
            unsafe { (*(&raw mut WAKERS)).push(waker.clone()) }
        }

        if let Some(v) = cmd.take() {
            unsafe {
                *cmd = write_cmd(v).err();
            }
        } else if waited && !first {
            return Poll::Ready(Ok(JsValue::undefined()));
        }

        Poll::Pending
    }
}
