use std::ops::Deref;

use boa_engine::object::ObjectInitializer;
use boa_engine::prelude::*;
use boa_engine::property::Attribute;
use boa_engine::{JsArgs as _, JsResult, js_string};

use level_state::CHUNK_SIZE;

use super::{block_to_str, get_level};

#[derive(Debug, Trace, JsData, Finalize)]
pub struct Chunk {
    #[unsafe_ignore_trace]
    pub x: usize,
    #[unsafe_ignore_trace]
    pub y: usize,
    #[unsafe_ignore_trace]
    pub z: usize,
}

impl Chunk {
    pub fn new_proto(ctx: &mut Context) -> JsObject {
        let mut builder = ObjectInitializer::new(ctx);

        builder.function(
            NativeFunction::from_copy_closure(Self::get_block),
            js_string!("getBlock"),
            0,
        );
        builder.property(
            js_string!("chunkSize"),
            CHUNK_SIZE,
            Attribute::ENUMERABLE | Attribute::READONLY,
        );

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

        let level = unsafe { get_level()?.1 };

        Ok(block_to_str(
            level
                .get_chunk(this.x, this.y, this.z)
                .get_block(x, y, z)
                .get(),
        )
        .into())
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
