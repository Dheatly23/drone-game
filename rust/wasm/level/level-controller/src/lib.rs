#![allow(clippy::deref_addrof)]

mod render;

use rkyv::api::high::{from_bytes, to_bytes_in_with_alloc};
use rkyv::rancor::Panic;
use rkyv::ser::allocator::Arena;
use rkyv::ser::writer::Buffer;

use level_state::{Block, LevelState, CHUNK_SIZE};
use util_wasm::{read, write};

use crate::render::{render_chunk, ExportRender};

static mut ARENA: Option<Arena> = None;
static mut LEVEL: LevelState = LevelState::new_empty();

#[unsafe(no_mangle)]
pub extern "C" fn init(x: u32, y: u32, z: u32) {
    let level = unsafe { &mut *(&raw mut LEVEL) };
    *level = LevelState::new_empty();
    *level = LevelState::new(x as _, y as _, z as _);

    // TODO: Testing data
    for z in 0..z as usize {
        for x in 0..x as usize {
            let c = level.get_chunk_mut(x, 0, z);
            let i = (x + z) & 1;
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    if i != 0 {
                        c.get_block_mut(x, 0, z).set(Block::Grass);
                    } else if (x + z) & 1 != 0 {
                        c.get_block_mut(x, 0, z).set(Block::Dirt);
                    }
                }
            }
        }
    }
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
            let (level, arena) = (&*(&raw const LEVEL), &mut *(&raw mut ARENA));
            to_bytes_in_with_alloc::<_, _, Panic>(
                level,
                Buffer::from(buf),
                arena.get_or_insert_default().acquire(),
            )
            .unwrap()
            .len()
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_chunk(x: u32, y: u32, z: u32) -> *const ExportRender {
    render_chunk(unsafe { &mut *(&raw mut LEVEL) }, x as _, y as _, z as _)
}
