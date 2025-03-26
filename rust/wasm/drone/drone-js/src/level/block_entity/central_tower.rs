use boa_engine::object::ObjectInitializer;
use boa_engine::prelude::*;
use boa_engine::property::Attribute;
use boa_engine::{JsResult, js_string};
use enumflags2::{BitFlags, make_bitflags};

use level_state::{ArchivedBlockEntityData, ArchivedCentralTower, DroneCapabilityFlags};

use super::drone::{cap_object, inventory_to_obj};
use super::{err_deleted, unwrap_data};

pub fn proto(ctx: &mut Context) -> JsObject {
    let mut builder = ObjectInitializer::new(ctx);

    builder.function(
        NativeFunction::from_copy_closure(get_inventory),
        js_string!("getInventory"),
        0,
    );
    builder.function(
        NativeFunction::from_copy_closure(get_capabilities),
        js_string!("getCapabilities"),
        0,
    );
    builder.function(
        NativeFunction::from_copy_closure(get_ext_inventory),
        js_string!("getExtInventory"),
        0,
    );
    builder.function(
        NativeFunction::from_copy_closure(has_ext_inventory),
        js_string!("hasExtInventory"),
        0,
    );
    builder.function(
        NativeFunction::from_copy_closure(has_move),
        js_string!("canMove"),
        0,
    );
    builder.function(
        NativeFunction::from_copy_closure(has_fly),
        js_string!("canFly"),
        0,
    );
    builder.function(
        NativeFunction::from_copy_closure(has_break),
        js_string!("canBreak"),
        0,
    );
    builder.function(
        NativeFunction::from_copy_closure(has_silk_touch),
        js_string!("breakSilkTouch"),
        0,
    );
    builder.function(
        NativeFunction::from_copy_closure(has_spawn),
        js_string!("canSpawn"),
        0,
    );

    let getter =
        NativeFunction::from_copy_closure(get_is_valid).to_js_function(builder.context().realm());
    builder.accessor(
        js_string!("commandValid"),
        Some(getter),
        None,
        Attribute::ENUMERABLE | Attribute::READONLY,
    );

    builder.build()
}

fn get_is_valid(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(unwrap_central_tower(this)?.is_command_valid.into())
}

fn get_capabilities(this: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    cap_object(
        unwrap_central_tower(this)?
            .capabilities
            .flags
            .into_bit_flags(),
        ctx,
    )
}

fn has_flag(this: &JsValue, flag: BitFlags<DroneCapabilityFlags>) -> JsResult<JsValue> {
    Ok(unwrap_central_tower(this)?
        .capabilities
        .flags
        .into_bit_flags()
        .contains(flag)
        .into())
}

fn has_move(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    has_flag(this, make_bitflags!(DroneCapabilityFlags::Moving))
}

fn has_fly(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    has_flag(this, make_bitflags!(DroneCapabilityFlags::Flying))
}

fn has_break(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    has_flag(this, make_bitflags!(DroneCapabilityFlags::Breaker))
}

fn has_silk_touch(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    has_flag(this, make_bitflags!(DroneCapabilityFlags::SilkTouch))
}

fn has_spawn(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    has_flag(this, make_bitflags!(DroneCapabilityFlags::DroneSummon))
}

fn get_inventory(this: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    inventory_to_obj(&unwrap_central_tower(this)?.inventory, ctx)
}

fn has_ext_inventory(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(unwrap_central_tower(this)?
        .capabilities
        .ext_inventory
        .is_some()
        .into())
}

fn get_ext_inventory(this: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    match &unwrap_central_tower(this)?
        .capabilities
        .ext_inventory
        .as_ref()
    {
        Some(v) => inventory_to_obj(v, ctx),
        None => Ok(JsValue::Null),
    }
}

fn unwrap_central_tower(this: &JsValue) -> JsResult<&ArchivedCentralTower> {
    match &unwrap_data(this)?.data {
        ArchivedBlockEntityData::CentralTower(v) => Ok(v),
        _ => Err(err_deleted()),
    }
}
