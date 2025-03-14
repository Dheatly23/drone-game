#![allow(clippy::deref_addrof)]

mod entity;
mod process_export;
mod render;
mod update;

use std::mem::replace;

use rkyv::api::high::{from_bytes, to_bytes_in};
use rkyv::rancor::{Failure, Panic};
use rkyv::ser::writer::Buffer;
use uuid::Uuid;

use level_state::{
    Block, BlockEntity, BlockEntityData, CHUNK_SIZE, CentralTower, IronOre, LevelState,
};
use util_wasm::{read, write};

use crate::entity::update_entity;
use crate::process_export::process_to_export;
use crate::render::{ExportRender, render_chunk};
use crate::update::update;

static mut LEVEL: LevelState = LevelState::new_empty();
static mut LEVEL_PROCESSED: LevelState = LevelState::new_empty();

#[unsafe(no_mangle)]
pub extern "C" fn init(x: u32, y: u32, z: u32) {
    let (level, level_processed) =
        unsafe { (&mut *(&raw mut LEVEL), &mut *(&raw mut LEVEL_PROCESSED)) };
    *level = LevelState::new_empty();
    *level_processed = LevelState::new_empty();
    *level = LevelState::new(x as _, y as _, z as _);
    *level_processed = LevelState::new(x as _, y as _, z as _);
}

#[unsafe(no_mangle)]
pub extern "C" fn import() {
    let (level, level_processed) =
        unsafe { (&mut *(&raw mut LEVEL), &mut *(&raw mut LEVEL_PROCESSED)) };
    *level = LevelState::new_empty();
    *level_processed = LevelState::new_empty();
    *level = from_bytes::<LevelState, Panic>(unsafe { read() }).unwrap();
    let (sx, sy, sz) = level.chunk_size();
    *level_processed = LevelState::new(sx, sy, sz);

    // Validation
    for c in level.chunks_mut() {
        for b in c.blocks_mut() {
            if matches!(b.get(), IronOre::BLOCK | CentralTower::BLOCK) {
                b.set(Block::Unknown);
            }
        }
    }

    let mut v = Vec::new();
    for (
        &k,
        &BlockEntity {
            x, y, z, ref data, ..
        },
    ) in level.block_entities().entries()
    {
        match data {
            BlockEntityData::IronOre(_) => v.push(((x, y, z), k, Some(IronOre::BLOCK))),
            BlockEntityData::Drone(_) => v.push(((x, y, z), k, None)),
            BlockEntityData::CentralTower(_) => {
                for x in (-1isize..2).filter_map(|d| x.checked_add_signed(d)) {
                    for z in (-1isize..2).filter_map(|d| z.checked_add_signed(d)) {
                        for y in (0isize..3).filter_map(|d| y.checked_add_signed(d)) {
                            v.push(((x, y, z), k, Some(CentralTower::BLOCK)));
                        }
                    }
                }
            }
            _ => v.push(((x, y, z), k, Some(Block::Unknown))),
        }
    }
    v.sort_unstable_by(|(ac, ai, ab), (bc, bi, bb)| {
        ac.cmp(bc)
            .then_with(|| ab.map(u16::from).cmp(&bb.map(u16::from)).reverse())
            .then_with(|| ai.cmp(bi))
    });

    let mut prev = None;
    for &(c @ (x, y, z), ref id, _) in &v {
        if x / CHUNK_SIZE >= sx
            || y / CHUNK_SIZE >= sy
            || z / CHUNK_SIZE >= sz
            || replace(&mut prev, Some(c)).is_some_and(|p| c == p)
        {
            level.block_entities_mut().remove(id);
        }
    }

    if let Some(min) = level
        .block_entities()
        .entries()
        .filter_map(|(&k, v)| match v.data {
            BlockEntityData::CentralTower(_) => Some(k),
            _ => None,
        })
        .min()
    {
        level
            .block_entities_mut()
            .remove_if(|k, v| *k != min && matches!(v.data, BlockEntityData::CentralTower(_)));
    }

    for ((x, y, z), id, b) in v {
        if level.block_entities().get(&id).is_none() {
            continue;
        }

        if let Some(b) = b {
            level.get_block_mut(x, y, z).set(b);
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
pub extern "C" fn export_censored() {
    let (level, level_processed) =
        unsafe { (&*(&raw const LEVEL), &mut *(&raw mut LEVEL_PROCESSED)) };
    process_to_export(level_processed, level);
    unsafe {
        write(|buf| {
            to_bytes_in::<_, Panic>(level_processed, Buffer::from(buf))
                .unwrap()
                .len()
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn set_command(a0: u32, a1: u32, a2: u32, a3: u32) {
    let id = Uuid::from_u128(
        (a0 as u128) | ((a1 as u128) << 32) | ((a2 as u128) << 64) | ((a3 as u128) << 96),
    );

    unsafe {
        let level = &mut *(&raw mut LEVEL);
        let Some(BlockEntity {
            data: BlockEntityData::Drone(d),
            ..
        }) = level.block_entities_mut().get_mut(&id)
        else {
            return;
        };
        if let Ok(v) = from_bytes::<_, Failure>(read()) {
            //log(format_args!("{id} {v:?}"));
            d.command = v;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn tick() {
    update(unsafe { &mut *(&raw mut LEVEL) });
}
