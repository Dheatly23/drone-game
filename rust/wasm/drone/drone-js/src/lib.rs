#![allow(clippy::deref_addrof)]

mod executor;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::env::args;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult, Write as _};
use std::future::Future;
use std::mem::{replace, swap};
use std::ops::Deref;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as FutContext, Poll, Waker};

use arrayvec::ArrayString;
use boa_engine::class::{Class, ClassBuilder};
use boa_engine::job::{FutureJob, JobQueue, NativeJob};
use boa_engine::object::builtins::{JsArray, JsPromise};
use boa_engine::object::{IntegrityLevel, ObjectInitializer};
use boa_engine::prelude::*;
use boa_engine::property::Attribute;
use boa_engine::{JsArgs as _, JsResult, js_error, js_string};
use boa_runtime::Console;
use rkyv::api::high::{access, to_bytes_in};
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;
use uuid::Uuid;

use level_state::{
    ArchivedBlockEntityData, ArchivedLevelState, Block, CHUNK_SIZE, Command, Direction,
};
use util_wasm::{read, write_data};

static mut UUID: Uuid = Uuid::nil();
static mut CONTEXT: Option<Context> = None;
static mut LEVEL: Option<&'static ArchivedLevelState> = None;
static mut WAKERS: Vec<Rc<Cell<Option<Waker>>>> = Vec::new();

#[repr(C, align(16))]
struct BufferData([u8; 256]);
static mut BUFFER: BufferData = BufferData([0; 256]);
static mut WRITTEN: bool = true;

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
    let Some(path) = args().nth(1) else {
        return;
    };
    *context = Some(load_js(path).unwrap());
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

fn load_js(path: String) -> JsResult<Context> {
    let mut ctx = Context::builder()
        .job_queue(Rc::new(JobRunner::new(2)))
        .build()?;

    // Classes
    ctx.register_global_class::<Chunk>()?;

    // Console
    let console = Console::init(&mut ctx);
    ctx.register_global_property(Console::NAME, console, Attribute::all())?;

    // Level
    let level = Level::new(&mut ctx).into_object(&mut ctx);
    ctx.register_global_property(js_string!("Level"), level, Attribute::all())?;

    // Eval
    ctx.eval(Source::from_filepath(&PathBuf::from(path)).map_err(JsError::from_rust)?)?;

    Ok(ctx)
}

fn js_str_small(s: JsStr<'_>) -> Option<ArrayString<32>> {
    let mut r = ArrayString::<32>::new();

    for c in char::decode_utf16(s.iter()) {
        r.try_push(c.ok()?).ok()?;
    }

    Some(r)
}

#[derive(Debug, Trace, JsData, Finalize)]
struct Level {
    chunk_cache: HashMap<[usize; 3], JsObject>,
}

impl Level {
    fn new(_: &mut Context) -> Self {
        Self {
            chunk_cache: HashMap::new(),
        }
    }

    fn into_object(self, ctx: &mut Context) -> JsObject {
        let mut builder = ObjectInitializer::with_native_data(self, ctx);
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
        unsafe {
            match &*(&raw const LEVEL) {
                Some(level) => Ok(level.chunk_size().0.into()),
                None => Err(JsError::from_rust(LevelUninitError)),
            }
        }
    }

    fn get_y(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        unsafe {
            match &*(&raw const LEVEL) {
                Some(level) => Ok(level.chunk_size().1.into()),
                None => Err(JsError::from_rust(LevelUninitError)),
            }
        }
    }

    fn get_z(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        unsafe {
            match &*(&raw const LEVEL) {
                Some(level) => Ok(level.chunk_size().2.into()),
                None => Err(JsError::from_rust(LevelUninitError)),
            }
        }
    }

    fn get_chunk(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let this = Self::downcast_this(this)?;
        let x = args.get_or_undefined(0).try_js_into::<usize>(ctx)?;
        let y = args.get_or_undefined(1).try_js_into::<usize>(ctx)?;
        let z = args.get_or_undefined(2).try_js_into::<usize>(ctx)?;

        let level = unsafe {
            match &*(&raw const LEVEL) {
                Some(v) => v,
                None => return Err(JsError::from_rust(LevelUninitError)),
            }
        };
        let (sx, sy, sz) = level.chunk_size();
        if x >= sx {
            return Err(JsError::from_rust(IndexOverflowError {
                axis: Axis::X,
                value: x,
                max: sx,
            }));
        } else if y >= sy {
            return Err(JsError::from_rust(IndexOverflowError {
                axis: Axis::Y,
                value: y,
                max: sy,
            }));
        } else if z >= sz {
            return Err(JsError::from_rust(IndexOverflowError {
                axis: Axis::Z,
                value: z,
                max: sz,
            }));
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
        let level = unsafe {
            match &*(&raw const LEVEL) {
                Some(v) => v,
                None => return Err(JsError::from_rust(LevelUninitError)),
            }
        };

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

        let level = unsafe {
            match &*(&raw const LEVEL) {
                Some(v) => v,
                None => return Err(JsError::from_rust(LevelUninitError)),
            }
        };

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

        let level = unsafe {
            match &*(&raw const LEVEL) {
                Some(v) => v,
                None => return Err(JsError::from_rust(LevelUninitError)),
            }
        };

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
            CommandFuture {
                cmd: if unsafe { write_cmd(&cmd) } {
                    None
                } else {
                    Some(cmd)
                },
                waker: Rc::new(Cell::new(None)),
                first: true,
            },
            ctx,
        )
        .into())
    }

    fn tick(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        Ok(JsPromise::from_future(
            CommandFuture {
                cmd: None,
                waker: Rc::new(Cell::new(None)),
                first: true,
            },
            ctx,
        )
        .into())
    }

    fn downcast_this(this: &JsValue) -> JsResult<JsObject<Self>> {
        if let Some(obj) = this.as_object() {
            if let Ok(ret) = obj.clone().downcast::<Self>() {
                return Ok(ret);
            }
        }

        Err(JsError::from_rust(ThisTypeError))
    }
}

#[derive(Debug, Trace, JsData, Finalize)]
struct Chunk {
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
        Err(JsError::from_rust(UnconstructibleError))
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

        match (x, y, z) {
            (v @ CHUNK_SIZE.., _, _) => Err(JsError::from_rust(IndexOverflowError {
                axis: Axis::X,
                value: v,
                max: CHUNK_SIZE,
            })),
            (_, v @ CHUNK_SIZE.., _) => Err(JsError::from_rust(IndexOverflowError {
                axis: Axis::Y,
                value: v,
                max: CHUNK_SIZE,
            })),
            (_, _, v @ CHUNK_SIZE..) => Err(JsError::from_rust(IndexOverflowError {
                axis: Axis::Z,
                value: v,
                max: CHUNK_SIZE,
            })),
            (x, y, z) => unsafe {
                let Some(level) = &*(&raw const LEVEL) else {
                    return Err(JsError::from_rust(LevelUninitError));
                };

                Ok(Self::from_block(
                    level
                        .get_chunk(this.x, this.y, this.z)
                        .get_block(x, y, z)
                        .get(),
                ))
            },
        }
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

        Err(JsError::from_rust(ThisTypeError))
    }
}

struct UnconstructibleError;

impl Debug for UnconstructibleError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "class is internal-use and cannot be manually constructed."
        )
    }
}

impl Display for UnconstructibleError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(self, f)
    }
}

impl Error for UnconstructibleError {}

struct ThisTypeError;

impl Debug for ThisTypeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "this object value is invalid.")
    }
}

impl Display for ThisTypeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(self, f)
    }
}

impl Error for ThisTypeError {}

struct LevelUninitError;

impl Debug for LevelUninitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "this object is not a chunk.")
    }
}

impl Display for LevelUninitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(self, f)
    }
}

impl Error for LevelUninitError {}

enum Axis {
    X,
    Y,
    Z,
}

impl Display for Axis {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "{}",
            match self {
                Self::X => "x",
                Self::Y => "y",
                Self::Z => "z",
            }
        )
    }
}

struct IndexOverflowError {
    axis: Axis,
    value: usize,
    max: usize,
}

impl Debug for IndexOverflowError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "axis {} with value {} is overflowing! (max: {})",
            self.axis, self.value, self.max
        )
    }
}

impl Display for IndexOverflowError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(self, f)
    }
}

impl Error for IndexOverflowError {}

struct JobRunner {
    jobs: RefCell<(VecDeque<NativeJob>, VecDeque<NativeJob>)>,
    executor: crate::executor::Executor<NativeJob>,
    n_loop: usize,
}

impl JobRunner {
    fn new(n_loop: usize) -> Self {
        Self {
            jobs: RefCell::new((VecDeque::new(), VecDeque::new())),
            executor: Default::default(),
            n_loop,
        }
    }
}

impl JobQueue for JobRunner {
    fn enqueue_promise_job(&self, job: NativeJob, _: &mut Context) {
        self.jobs.borrow_mut().1.push_back(job);
    }

    fn enqueue_future_job(&self, future: FutureJob, ctx: &mut Context) {
        if let Some(job) = self.executor.register(future) {
            self.enqueue_promise_job(job, ctx);
        }
    }

    fn run_jobs(&self, ctx: &mut Context) {
        for _ in 0..self.n_loop {
            for job in self.executor.run() {
                self.enqueue_promise_job(job, ctx);
            }

            {
                let mut guard = self.jobs.borrow_mut();
                let (a, b) = &mut *guard;
                swap(a, b);
            }

            loop {
                let Some(job) = self.jobs.borrow_mut().0.pop_front() else {
                    break;
                };

                if let Err(e) = job.call(ctx) {
                    eprintln!("Error in promise: {e}");
                }
            }
        }
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
    waker: Rc<Cell<Option<Waker>>>,
    first: bool,
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
