#![allow(clippy::deref_addrof)]

mod render;

use rkyv::api::high::{from_bytes, to_bytes_in};
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;

use level_state::LevelState;
use util_wasm::{read, write};

use crate::render::{render_chunk, ExportRender};

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
    unsafe {
        write(|buf| {
            to_bytes_in::<_, Panic>(&*(&raw const LEVEL), Buffer::from(buf))
                .unwrap()
                .len()
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_chunk_x() -> u32 {
    unsafe { (*(&raw const LEVEL)).chunk_size().0 as _ }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_chunk_y() -> u32 {
    unsafe { (*(&raw const LEVEL)).chunk_size().1 as _ }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_chunk_z() -> u32 {
    unsafe { (*(&raw const LEVEL)).chunk_size().2 as _ }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_chunk(x: u32, y: u32, z: u32) -> *const ExportRender {
    render_chunk(unsafe { &mut *(&raw mut LEVEL) }, x as _, y as _, z as _)
}
