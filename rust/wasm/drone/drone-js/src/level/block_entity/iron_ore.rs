use boa_engine::object::ObjectInitializer;
use boa_engine::prelude::*;
use boa_engine::property::Attribute;
use boa_engine::{JsResult, js_string};

use level_state::{ArchivedBlockEntityData, ArchivedIronOre};

use super::{err_deleted, unwrap_data};

pub fn proto(ctx: &mut Context) -> JsObject {
    let mut builder = ObjectInitializer::new(ctx);

    let getter =
        NativeFunction::from_copy_closure(get_quantity).to_js_function(builder.context().realm());
    builder.accessor(
        js_string!("quantity"),
        Some(getter),
        None,
        Attribute::ENUMERABLE | Attribute::READONLY,
    );

    builder.build()
}

fn get_quantity(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(unwrap_iron_ore(this)?.quantity.to_native().into())
}

fn unwrap_iron_ore(this: &JsValue) -> JsResult<&ArchivedIronOre> {
    match &unwrap_data(this)?.data {
        ArchivedBlockEntityData::IronOre(v) => Ok(v),
        _ => Err(err_deleted()),
    }
}
