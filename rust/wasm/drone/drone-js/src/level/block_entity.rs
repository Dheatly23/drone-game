use std::ptr::NonNull;

use boa_engine::object::builtins::JsArray;
use boa_engine::object::{IntegrityLevel, ObjectInitializer};
use boa_engine::prelude::*;
use boa_engine::property::{Attribute, PropertyDescriptor};
use boa_engine::{JsResult, js_string};
use uuid::Uuid;

use level_state::{
    ArchivedBlockEntity, ArchivedBlockEntityData, ArchivedDrone, ArchivedIronOre, Item,
};

use super::{check_level, get_level, item_to_str};

#[derive(Debug, Trace, JsData, Finalize)]
pub struct Wrapper {
    default_proto: JsObject,
    iron_ore_proto: JsObject,
    drone_proto: JsObject,
}

impl Wrapper {
    pub fn new_proto(ctx: &mut Context) -> Self {
        let default_proto = WrapperData::default_proto(ctx);
        let iron_ore_proto = WrapperData::iron_ore_proto(ctx);
        iron_ore_proto.set_prototype(Some(default_proto.clone()));
        let drone_proto = WrapperData::drone_proto(ctx);
        drone_proto.set_prototype(Some(default_proto.clone()));

        Self {
            default_proto,
            iron_ore_proto,
            drone_proto,
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
                .value(JsString::from(uuid.hyphenated().to_string()))
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
        Ok(Self::unwrap_data(this)?.x.to_native().into())
    }

    fn get_y(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(Self::unwrap_data(this)?.y.to_native().into())
    }

    fn get_z(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(Self::unwrap_data(this)?.z.to_native().into())
    }

    fn unwrap_data(this: &JsValue) -> JsResult<&ArchivedBlockEntity> {
        let Some(mut this) = this.as_object().and_then(|v| v.downcast_mut::<Self>()) else {
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
                    None => Err(Self::err_deleted()),
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
}

impl WrapperData {
    fn iron_ore_proto(ctx: &mut Context) -> JsObject {
        let mut builder = ObjectInitializer::new(ctx);

        let getter = NativeFunction::from_copy_closure(Self::get_quantity)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("quantity"),
            Some(getter),
            None,
            Attribute::ENUMERABLE | Attribute::READONLY,
        );

        builder.build()
    }

    fn get_quantity(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(Self::unwrap_iron_ore(this)?.quantity.to_native().into())
    }

    fn unwrap_iron_ore(this: &JsValue) -> JsResult<&ArchivedIronOre> {
        match &Self::unwrap_data(this)?.data {
            ArchivedBlockEntityData::IronOre(v) => Ok(v),
            _ => Err(Self::err_deleted()),
        }
    }
}

impl WrapperData {
    fn drone_proto(ctx: &mut Context) -> JsObject {
        let mut builder = ObjectInitializer::new(ctx);

        builder.function(
            NativeFunction::from_copy_closure(Self::get_inventory),
            js_string!("getInventory"),
            0,
        );

        let getter = NativeFunction::from_copy_closure(Self::get_is_valid)
            .to_js_function(builder.context().realm());
        builder.accessor(
            js_string!("commandValid"),
            Some(getter),
            None,
            Attribute::ENUMERABLE | Attribute::READONLY,
        );

        builder.build()
    }

    fn get_is_valid(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        Ok(Self::unwrap_drone(this)?.is_command_valid.into())
    }

    fn get_inventory(this: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let inv = &Self::unwrap_drone(this)?.inventory;
        let mut r = [const { None }; 9 * 3];
        assert_eq!(r.len(), inv.len());
        for i in 0..inv.len() {
            let v = &inv[i];
            let item = v.item();
            if item == Item::Air {
                continue;
            }

            let t = ObjectInitializer::new(ctx)
                .property(
                    js_string!("type"),
                    item_to_str(item),
                    Attribute::ENUMERABLE | Attribute::READONLY,
                )
                .property(
                    js_string!("count"),
                    v.count(),
                    Attribute::ENUMERABLE | Attribute::READONLY,
                )
                .build();
            t.set_integrity_level(IntegrityLevel::Frozen, ctx)?;
            r[i] = Some(t);
        }

        Ok(JsArray::from_iter(
            r.into_iter().map(|v| match v {
                Some(v) => JsValue::Object(v),
                None => JsValue::Null,
            }),
            ctx,
        )
        .into())
    }

    fn unwrap_drone(this: &JsValue) -> JsResult<&ArchivedDrone> {
        match &Self::unwrap_data(this)?.data {
            ArchivedBlockEntityData::Drone(v) => Ok(v),
            _ => Err(Self::err_deleted()),
        }
    }
}
