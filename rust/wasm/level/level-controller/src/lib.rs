#![allow(clippy::deref_addrof)]

mod entity;
mod render;
mod update;

use std::mem::replace;

use rkyv::api::high::{from_bytes, to_bytes_in};
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;

use level_state::{Block, BlockEntityData, IronOre, LevelState, CHUNK_SIZE};
use util_wasm::{read, write};

use crate::entity::update_entity;
use crate::render::{render_chunk, ExportRender};
use crate::update::update;

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

    // Validation
    for c in level.chunks_mut() {
        for b in c.blocks_mut() {
            if matches!(b.get(), IronOre::BLOCK) {
                b.set(Block::Unknown);
            }
        }
    }

    let mut v: Vec<_> = level
        .block_entities()
        .entries()
        .map(|(&k, v)| {
            (
                (v.x, v.y, v.z),
                k,
                match v.data {
                    BlockEntityData::IronOre(_) => Some(IronOre::BLOCK),
                    BlockEntityData::Drone(_) => None,
                    _ => Some(Block::Unknown),
                },
            )
        })
        .collect();
    v.sort_unstable_by(|(a, _, _), (b, _, _)| a.cmp(b));

    let (sx, sy, sz) = level.chunk_size();
    let mut prev = None;
    for (c @ (x, y, z), id, b) in v {
        if x >= sx || y >= sy || z >= sz || replace(&mut prev, Some(c)).is_some_and(|p| c == p) {
            level.block_entities_mut().remove(&id);
            continue;
        }

        if let Some(b) = b {
            level
                .get_chunk_mut(x / CHUNK_SIZE, y / CHUNK_SIZE, z / CHUNK_SIZE)
                .get_block_mut(x % CHUNK_SIZE, y % CHUNK_SIZE, z % CHUNK_SIZE)
                .set(b);
        }
    }

    for _ in level.block_entities_mut().pop_removed() {}
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

#[unsafe(no_mangle)]
pub extern "C" fn entity_update() {
    update_entity(unsafe { &mut *(&raw mut LEVEL) });
}

#[unsafe(no_mangle)]
pub extern "C" fn tick() {
    update(unsafe { &mut *(&raw mut LEVEL) });
}
