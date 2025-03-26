mod central_tower;
mod drone;
mod iron_ore;

use std::ptr::NonNull;

use boa_engine::object::{IntegrityLevel, ObjectInitializer};
use boa_engine::prelude::*;
use boa_engine::property::{Attribute, PropertyDescriptor};
use boa_engine::{JsResult, js_string};
use enumflags2::{BitFlags, make_bitflags};
use uuid::Uuid;

use level_state::{ArchivedBlockEntity, ArchivedBlockEntityData, DroneCapabilityFlags};

use super::{check_level, get_level};
use crate::util::format_uuid;
use central_tower::proto as central_tower_proto;
use drone::proto as drone_proto;
use iron_ore::proto as iron_ore_proto;

#[derive(Debug, Trace, JsData, Finalize)]
pub struct Wrapper {
    default_proto: JsObject,
    iron_ore_proto: JsObject,
    drone_proto: JsObject,
    central_tower_proto: JsObject,
}

impl Wrapper {
    pub fn new_proto(ctx: &mut Context) -> Self {
        let default_proto = WrapperData::default_proto(ctx);
        let iron_ore_proto = iron_ore_proto(ctx);
        iron_ore_proto.set_prototype(Some(default_proto.clone()));
        let drone_proto = drone_proto(ctx);
        drone_proto.set_prototype(Some(default_proto.clone()));
        let central_tower_proto = central_tower_proto(ctx);
        central_tower_proto.set_prototype(Some(default_proto.clone()));

        Self {
            default_proto,
            iron_ore_proto,
            drone_proto,
            central_tower_proto,
        }
    }

    pub fn build(
        &self,
        uuid: Uuid,
        be: &ArchivedBlockEntity,
        ctx: &mut Context,
    ) -> JsResult<JsObject<WrapperData>> {
        let epoch = unsafe { get_level()?.0 };
        let ret = JsObject::new(
            ctx.root_shape(),
            match be.data {
                ArchivedBlockEntityData::IronOre(_) => &self.iron_ore_proto,
                ArchivedBlockEntityData::Drone(_) => &self.drone_proto,
                ArchivedBlockEntityData::CentralTower(_) => &self.central_tower_proto,
                _ => &self.default_proto,
            }
            .clone(),
            WrapperData {
                uuid,
                epoch,
                ptr: be.into(),
            },
        );
        ret.insert_property(
            js_string!("uuid"),
            PropertyDescriptor::builder()
                .value(format_uuid(&uuid))
                .enumerable(true),
        );
        ret.clone()
            .upcast()
            .set_integrity_level(IntegrityLevel::Frozen, ctx)?;
        Ok(ret)
    }
}

#[derive(Debug, Trace, JsData, Finalize)]
pub struct WrapperData {
    #[unsafe_ignore_trace]
    uuid: Uuid,
    #[unsafe_ignore_trace]
    epoch: u64,
    #[unsafe_ignore_trace]
    ptr: NonNull<ArchivedBlockEntity>,
}

impl WrapperData {
    fn default_proto(ctx: &mut Context) -> JsObject {
        let mut builder = ObjectInitializer::new(ctx);

        let getter = NativeFunction::from_copy_closure(Self::get_x)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("x"),
            Some(getter),
            None,
            Attribute::ENUMERABLE | Attribute::READONLY,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_y)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("y"),
            Some(getter),
            None,
            Attribute::ENUMERABLE | Attribute::READONLY,
        );
        let getter = NativeFunction::from_copy_closure(Self::get_z)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("z"),
            Some(getter),
            None,
            Attribute::ENUMERABLE | Attribute::READONLY,
        );

        builder.build()
    }

    fn get_x(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(unwrap_data(this)?.x.to_native().into())
    }

    fn get_y(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(unwrap_data(this)?.y.to_native().into())
    }

    fn get_z(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(unwrap_data(this)?.z.to_native().into())
    }
}

fn unwrap_data(this: &JsValue) -> JsResult<&ArchivedBlockEntity> {
    let Some(mut this) = this
        .as_object()
        .and_then(|v| v.downcast_mut::<WrapperData>())
    else {
        return Err(JsNativeError::typ()
            .with_message("invalid this object type")
            .into());
    };
    let this = &mut *this;

    unsafe {
        let p = *check_level(&mut this.epoch, &mut this.ptr, |level, p| {
            match level.block_entities().get(&this.uuid) {
                Some(v) => {
                    *p = v.into();
                    Ok(())
                }
                None => Err(err_deleted()),
            }
        })?;
        Ok(&*p.as_ptr())
    }
}

fn err_deleted() -> JsError {
    JsNativeError::typ()
        .with_message("block entity is deleted")
        .into()
}

pub static CAP_FLAGS_LIST: &[(JsStr<'static>, BitFlags<DroneCapabilityFlags>)] = &[
    (
        js_str!("move"),
        make_bitflags!(DroneCapabilityFlags::Moving),
    ),
    (js_str!("fly"), make_bitflags!(DroneCapabilityFlags::Flying)),
    (
        js_str!("break"),
        make_bitflags!(DroneCapabilityFlags::Breaker),
    ),
    (
        js_str!("silkTouch"),
        make_bitflags!(DroneCapabilityFlags::SilkTouch),
    ),
    (
        js_str!("backpack"),
        make_bitflags!(DroneCapabilityFlags::ExtendedInventory),
    ),
    (
        js_str!("spawn"),
        make_bitflags!(DroneCapabilityFlags::DroneSummon),
    ),
];
