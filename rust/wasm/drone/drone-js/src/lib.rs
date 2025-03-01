#![allow(clippy::deref_addrof)]

mod executor;
mod level;
mod module;

use std::env::vars_os;
use std::ffi::{OsStr, OsString};
use std::fmt::Debug;
use std::path::PathBuf;
use std::rc::Rc;

use arrayvec::ArrayString;
use boa_engine::object::ObjectInitializer;
use boa_engine::object::builtins::{JsArray, JsMap};
use boa_engine::prelude::*;
use boa_engine::property::Attribute;
use boa_engine::{JsResult, js_string};
use boa_runtime::Console;
use clap::Parser;
use rkyv::api::high::access;
use rkyv::rancor::Panic;
use uuid::Uuid;

use level_state::ArchivedLevelState;
use util_wasm::read;

use crate::level::{LEVEL, WAKERS, WRITTEN};

/// JS runner for drone.
#[derive(Debug, Parser)]
#[command(version, about, long_about)]
struct Args {
    /// JS file to run.
    file: PathBuf,

    /// Arguments for JS.
    extra: Vec<OsString>,

    /// Run in strict mode.
    #[arg(long)]
    strict: bool,

    /// Run file as module.
    #[arg(short, long)]
    module: bool,

    /// Root for module resolution.
    #[arg(short, long)]
    root: Option<PathBuf>,
}

static mut UUID: Uuid = Uuid::nil();
static mut CONTEXT: Option<Context> = None;

#[repr(C, align(16))]
struct BufferData([u8; 256]);
static mut BUFFER: BufferData = BufferData([0; 256]);

#[unsafe(no_mangle)]
pub extern "C" fn init(a0: u32, a1: u32, a2: u32, a3: u32) {
    let context;
    unsafe {
        *(&raw mut UUID) = Uuid::from_u128(
            (a0 as u128) | ((a1 as u128) << 32) | ((a2 as u128) << 64) | ((a3 as u128) << 96),
        );
        context = &mut *(&raw mut CONTEXT);
    }

    *context = None;
    *context = Some(load_js(Args::parse()).unwrap());
}

#[unsafe(no_mangle)]
pub extern "C" fn tick() {
    unsafe {
        *(&raw mut LEVEL) = None;
        *(&raw mut LEVEL) = Some(access::<ArchivedLevelState, Panic>(read()).unwrap());
        *(&raw mut WRITTEN) = false;

        while let Some(w) = (*(&raw mut WAKERS)).pop() {
            if let Some(w) = w.take() {
                w.wake();
            }
        }

        if let Some(ctx) = &mut *(&raw mut CONTEXT) {
            ctx.run_jobs();
        }

        *(&raw mut LEVEL) = None;
    }
}

fn load_js(
    Args {
        file,
        extra,
        strict,
        module,
        root,
    }: Args,
) -> JsResult<Context> {
    assert!(file.is_absolute(), "relative path is not allowed!");

    let loader = Rc::new(crate::module::ModLoader::new(
        root.unwrap_or_else(|| PathBuf::from("/")),
    ));
    let mut ctx = Context::builder()
        .job_queue(Rc::new(crate::executor::JobRunner::new(16)))
        .module_loader(loader.clone())
        .build()?;
    ctx.strict(strict);

    // Classes
    ctx.register_global_class::<crate::level::Chunk>()?;

    // Console
    let console = Console::init(&mut ctx);
    ctx.register_global_property(Console::NAME, console, Attribute::all())?;

    // OS
    let mut temp = Vec::new();
    let mut f = move |s: &OsStr| {
        temp.clear();
        for c in s.as_encoded_bytes().utf8_chunks() {
            temp.extend(c.valid().encode_utf16());
            if !c.invalid().is_empty() {
                temp.push(0xfffd);
            }
        }

        JsString::from(&*temp)
    };
    let argv = JsArray::from_iter(extra.into_iter().map(|s| f(&s).into()), &mut ctx);

    let env = JsMap::new(&mut ctx);
    for (k, v) in vars_os() {
        env.set(f(&k), f(&v), &mut ctx)?;
    }

    drop(f);
    let os = ObjectInitializer::new(&mut ctx)
        .property(js_string!("argv"), argv, Attribute::all())
        .property(js_string!("env"), env, Attribute::all())
        .build();
    ctx.register_global_property(js_string!("OS"), os, Attribute::all())?;

    // Level
    let level = crate::level::Level::new_object(&mut ctx);
    ctx.register_global_property(js_string!("Level"), level, Attribute::all())?;

    if module {
        // Module
        let module = Module::parse(
            Source::from_filepath(&file).map_err(JsError::from_rust)?,
            None,
            &mut ctx,
        )?;
        loader.insert(file.clone(), module.clone());
        // We do not care about module result
        module.load_link_evaluate(&mut ctx);
    } else {
        // Eval
        ctx.eval(Source::from_filepath(&file).map_err(JsError::from_rust)?)?;
    }

    Ok(ctx)
}

fn js_str_small(s: JsStr<'_>) -> Option<ArrayString<32>> {
    let mut r = ArrayString::<32>::new();

    for c in char::decode_utf16(s.iter()) {
        r.try_push(c.ok()?).ok()?;
    }

    Some(r)
}
