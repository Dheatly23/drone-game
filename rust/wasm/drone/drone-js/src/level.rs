use std::cell::{Cell, RefCell};
use std::collections::hash_map::{Entry, HashMap};
use std::fmt::{Debug, Write as _};
use std::future::Future;
use std::mem::{MaybeUninit, replace};
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as FutContext, Poll, Waker};

use boa_engine::class::{Class, ClassBuilder};
use boa_engine::job::NativeJob;
use boa_engine::object::builtins::{JsArray, JsArrayBuffer, JsFunction, JsPromise};
use boa_engine::object::{IntegrityLevel, ObjectInitializer};
use boa_engine::prelude::*;
use boa_engine::property::Attribute;
use boa_engine::{JsArgs as _, JsResult, js_error, js_string};
use rkyv::api::high::to_bytes_in;
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;
use uuid::Uuid;

use level_state::{
    ArchivedBlockEntityData, ArchivedLevelState, Block, CHUNK_SIZE, Command, Direction,
};
use util_wasm::{ChannelId, write_data};

use crate::{BUFFER, UUID, js_str_small};

type WakerCell = Rc<Cell<Option<Waker>>>;

pub static mut LEVEL: Option<&'static ArchivedLevelState> = None;
pub static mut WAKERS: Vec<Rc<Cell<Option<Waker>>>> = Vec::new();
pub static mut WRITTEN: bool = true;

#[derive(Debug, Trace, Finalize)]
struct SubscriberCb {
    func: Option<JsFunction>,
    #[unsafe_ignore_trace]
    channel: ChannelId,
}

#[derive(Debug, Trace, JsData, Finalize)]
pub struct Level {
    chunk_cache: HashMap<[usize; 3], JsObject>,
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
                chunk_cache: HashMap::new(),
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
        builder.property(
            js_string!("uuid"),
            JsString::from(unsafe { (*(&raw const UUID)).as_hyphenated().to_string() }),
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );

        let getter = NativeFunction::from_copy_closure(Self::is_initialized)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("initialized"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_x)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("x"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_y)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("y"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_z)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("z"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );
        builder.build()
    }

    fn is_initialized(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        unsafe { Ok((*(&raw const LEVEL)).is_some().into()) }
    }

    fn get_x(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        let level = unsafe { Self::get_level()? };
        Ok(level.chunk_size().0.into())
    }

    fn get_y(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        let level = unsafe { Self::get_level()? };
        Ok(level.chunk_size().1.into())
    }

    fn get_z(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        let level = unsafe { Self::get_level()? };
        Ok(level.chunk_size().2.into())
    }

    fn get_chunk(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let x = args.get_or_undefined(0).try_js_into::<usize>(ctx)?;
        let y = args.get_or_undefined(1).try_js_into::<usize>(ctx)?;
        let z = args.get_or_undefined(2).try_js_into::<usize>(ctx)?;

        let level = unsafe { Self::get_level()? };

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

        let k = [x, y, z];
        let v = this.borrow().data().chunk_cache.get(&k).cloned();
        Ok(match v {
            Some(v) => v,
            None => {
                let c = Chunk::from_data(Chunk { x, y, z }, ctx)?;
                c.set_integrity_level(IntegrityLevel::Frozen, ctx)?;
                this.borrow_mut()
                    .data_mut()
                    .chunk_cache
                    .insert(k, c.clone());
                c
            }
        }
        .into())
    }

    fn get_block_entity_uuids(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let level = unsafe { Self::get_level()? };

        let mut s = String::new();
        Ok(JsArray::from_iter(
            level.block_entities().keys().filter_map(|k| {
                s.clear();
                write!(s, "{}", k.as_hyphenated()).ok()?;
                Some(JsString::from(&*s).into())
            }),
            ctx,
        )
        .into())
    }

    fn get_block_entity_uuids_at(
        _: &JsValue,
        args: &[JsValue],
        ctx: &mut Context,
    ) -> JsResult<JsValue> {
        let x = args.get_or_undefined(0).try_js_into::<usize>(ctx)?;
        let y = args.get_or_undefined(1).try_js_into::<usize>(ctx)?;
        let z = args.get_or_undefined(2).try_js_into::<usize>(ctx)?;

        let level = unsafe { Self::get_level()? };

        let mut s = String::new();
        Ok(JsArray::from_iter(
            level.block_entities().entries().filter_map(|(k, v)| {
                if v.x.to_native() as usize != x
                    || v.y.to_native() as usize != y
                    || v.z.to_native() as usize != z
                {
                    return None;
                }

                s.clear();
                write!(s, "{}", k.as_hyphenated()).ok()?;
                Some(JsString::from(&*s).into())
            }),
            ctx,
        )
        .into())
    }

    fn get_block_entity(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let uuid = Uuid::try_parse(
            &args
                .get_or_undefined(0)
                .try_js_into::<JsString>(ctx)?
                .to_std_string_lossy(),
        )
        .map_err(JsError::from_rust)?;

        let level = unsafe { Self::get_level()? };

        let Some(be) = level.block_entities().get(&uuid) else {
            return Ok(JsValue::null());
        };
        let mut builder = ObjectInitializer::new(ctx);
        builder.property(js_string!("x"), be.x.to_native(), Attribute::all());
        builder.property(js_string!("y"), be.y.to_native(), Attribute::all());
        builder.property(js_string!("z"), be.z.to_native(), Attribute::all());
        match &be.data {
            ArchivedBlockEntityData::IronOre(v) => {
                builder.property(js_string!("type"), js_string!("iron_ore"), Attribute::all());
                builder.property(
                    js_string!("quantity"),
                    v.quantity.to_native(),
                    Attribute::all(),
                );
            }
            ArchivedBlockEntityData::Drone(v) => {
                let v = v.get();
                builder.property(js_string!("type"), js_string!("drone"), Attribute::all());
                builder.property(js_string!("isValid"), v.is_command_valid, Attribute::all());
            }
            _ => {
                builder.property(js_string!("type"), js_string!("unknown"), Attribute::all());
            }
        }

        Ok(builder.build().into())
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

        let cmd = match js_str_small(
            cmd.get(js_string!("command"), ctx)?
                .try_js_into::<JsString>(ctx)?
                .as_str(),
        )
        .as_deref()
        {
            Some("noop") => Some(Command::Noop),
            Some("move") => match js_str_small(
                cmd.get(js_string!("direction"), ctx)?
                    .try_js_into::<JsString>(ctx)?
                    .as_str(),
            )
            .as_deref()
            {
                Some("up") => Some(Command::Move(Direction::Up)),
                Some("down") => Some(Command::Move(Direction::Down)),
                Some("left") => Some(Command::Move(Direction::Left)),
                Some("right") => Some(Command::Move(Direction::Right)),
                Some("forward") => Some(Command::Move(Direction::Forward)),
                Some("backward") => Some(Command::Move(Direction::Back)),
                _ => None,
            },
            _ => None,
        }
        .ok_or_else(|| js_error!("invalid command"))?;

        Ok(JsPromise::from_future(
            CommandFuture::new(if unsafe { write_cmd(&cmd) } {
                None
            } else {
                Some(cmd)
            }),
            ctx,
        )
        .into())
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

    unsafe fn get_level<'a>() -> JsResult<&'a ArchivedLevelState> {
        match unsafe { &*(&raw const LEVEL) } {
            Some(v) => Ok(v),
            None => Err(JsNativeError::error()
                .with_message("level is not yet initialized.")
                .into()),
        }
    }
}

#[derive(Debug, Trace, JsData, Finalize)]
pub struct Chunk {
    #[unsafe_ignore_trace]
    x: usize,
    #[unsafe_ignore_trace]
    y: usize,
    #[unsafe_ignore_trace]
    z: usize,
}

impl Class for Chunk {
    const NAME: &'static str = "Chunk";

    fn init(builder: &mut ClassBuilder<'_>) -> JsResult<()> {
        builder.method(
            js_string!("getBlock"),
            0,
            NativeFunction::from_copy_closure(Self::get_block),
        );
        builder.static_property(
            js_string!("chunkSize"),
            CHUNK_SIZE,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );

        let getter = NativeFunction::from_copy_closure(Self::get_x)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("x"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_y)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("y"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_z)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("z"),
            Some(getter),
            None,
            Attribute::CONFIGURABLE | Attribute::ENUMERABLE,
        );

        Ok(())
    }

    fn data_constructor(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<Self> {
        Err(JsNativeError::typ()
            .with_message("chunk is unconstructible")
            .into())
    }
}

impl Chunk {
    fn get_x(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(Self::downcast_this(this)?.x.into())
    }

    fn get_y(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(Self::downcast_this(this)?.y.into())
    }

    fn get_z(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(Self::downcast_this(this)?.z.into())
    }

    fn get_block(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let x = args.get_or_undefined(0).try_js_into::<usize>(ctx)?;
        let y = args.get_or_undefined(1).try_js_into::<usize>(ctx)?;
        let z = args.get_or_undefined(2).try_js_into::<usize>(ctx)?;

        if x >= CHUNK_SIZE {
            return Err(JsNativeError::typ()
                .with_message(format!("index x out of bounds ({x} >= {CHUNK_SIZE})"))
                .into());
        } else if y >= CHUNK_SIZE {
            return Err(JsNativeError::typ()
                .with_message(format!("index y out of bounds ({y} >= {CHUNK_SIZE})"))
                .into());
        } else if z >= CHUNK_SIZE {
            return Err(JsNativeError::typ()
                .with_message(format!("index z out of bounds ({z} >= {CHUNK_SIZE})"))
                .into());
        }

        let level = unsafe { Level::get_level()? };

        Ok(Self::from_block(
            level
                .get_chunk(this.x, this.y, this.z)
                .get_block(x, y, z)
                .get(),
        ))
    }

    fn from_block(b: Block) -> JsValue {
        match b {
            Block::Air => js_str!("air"),
            Block::Dirt => js_str!("dirt"),
            Block::Grass => js_str!("grass"),
            Block::IronOre => js_str!("iron_ore"),
            _ => js_str!("unknown"),
        }
        .into()
    }

    fn downcast_this(this: &JsValue) -> JsResult<impl '_ + Deref<Target = Self>> {
        if let Some(obj) = this.as_object() {
            if let Some(ret) = obj.downcast_ref::<Self>() {
                return Ok(ret);
            }
        }

        Err(JsNativeError::typ()
            .with_message("invalid this object type")
            .into())
    }
}

unsafe fn write_cmd(cmd: &Command) -> bool {
    unsafe {
        let written = &mut *(&raw mut WRITTEN);
        if *written {
            return false;
        }
        *written = true;

        let buffer = to_bytes_in::<_, Panic>(cmd, Buffer::from(&mut *(&raw mut BUFFER.0))).unwrap();
        write_data(buffer.as_ptr(), buffer.len() as _);
        true
    }
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

        if let Some(v) = &*cmd {
            if unsafe { write_cmd(v) } {
                *cmd = None;
            }
        } else if waited && !first {
            return Poll::Ready(Ok(JsValue::undefined()));
        }

        Poll::Pending
    }
}
