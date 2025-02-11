#![allow(clippy::deref_addrof)]

use rkyv::api::high::{from_bytes, to_bytes_in_with_alloc};
use rkyv::rancor::Panic;
use rkyv::ser::allocator::Arena;
use rkyv::ser::writer::Buffer;

use level_state::LevelState;

#[link(wasm_import_module = "host")]
extern "C" {
    #[link_name = "read_data"]
    fn _read_data(p: *mut u8, n: u64) -> u64;
    #[link_name = "write_data"]
    fn _write_data(p: *const u8, n: u64);
}

const BUF_LEN: usize = 1024 * 1024;
#[repr(C, align(65536))]
struct StaticBuffer([u8; BUF_LEN]);
static mut BUFFER: StaticBuffer = StaticBuffer([0; BUF_LEN]);
static mut ARENA: Option<Arena> = None;

unsafe fn read() -> &'static [u8] {
    let i = _read_data(&raw mut BUFFER.0 as _, BUF_LEN as _);
    &*(&raw const BUFFER.0[..i as usize])
}

static mut LEVEL: LevelState = LevelState::new_empty();

#[unsafe(no_mangle)]
pub extern "C" fn init(x: u32, y: u32, z: u32) {
    let level = unsafe { &mut *(&raw mut LEVEL) };
    *level = LevelState::new_empty();
    *level = LevelState::new(x as _, y as _, z as _);
}

#[unsafe(no_mangle)]
pub extern "C" fn import() {
    let level = unsafe { &mut *(&raw mut LEVEL) };
    *level = LevelState::new_empty();
    *level = from_bytes::<LevelState, Panic>(unsafe { read() }).unwrap();
}

#[unsafe(no_mangle)]
pub extern "C" fn export() {
    let (level, buf, arena) = unsafe {
        (
            &*(&raw const LEVEL),
            &mut *(&raw mut BUFFER.0),
            &mut *(&raw mut ARENA),
        )
    };
    let buf = to_bytes_in_with_alloc::<_, _, Panic>(
        level,
        Buffer::from(buf),
        arena.get_or_insert_default().acquire(),
    )
    .unwrap();
    unsafe {
        _write_data(buf.as_ptr(), buf.len() as _);
    }
}
