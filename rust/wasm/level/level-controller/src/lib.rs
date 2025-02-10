#![allow(clippy::deref_addrof)]

use rkyv::api::high::{from_bytes, to_bytes_in};
use rkyv::rancor::Panic;
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
static mut BUFFER: [u8; BUF_LEN] = [0; BUF_LEN];

unsafe fn read() -> &'static [u8] {
    let i = _read_data(&raw mut BUFFER as _, BUF_LEN as _);
    &*(&raw const BUFFER[..i as usize])
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
    let (level, buf) = unsafe { (&*(&raw const LEVEL), &mut *(&raw mut BUFFER)) };
    let buf = to_bytes_in::<_, Panic>(level, Buffer::from(buf)).unwrap();
    unsafe {
        _write_data(buf.as_ptr(), buf.len() as _);
    }
}
