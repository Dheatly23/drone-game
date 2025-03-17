#![allow(clippy::deref_addrof)]

mod entity;
mod process_export;
mod render;
mod update;
mod util;

use rand::prelude::*;
use rand_xoshiro::Xoshiro256PlusPlus;
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
static mut RNG: Option<Xoshiro256PlusPlus> = None;

unsafe fn get_rng<'a>() -> &'a mut Xoshiro256PlusPlus {
    unsafe { (*(&raw mut RNG)).get_or_insert_with(Xoshiro256PlusPlus::from_os_rng) }
}

#[unsafe(no_mangle)]
pub extern "C" fn init(x: u32, y: u32, z: u32) {
    let (level, level_processed) =
        unsafe { (&mut *(&raw mut LEVEL), &mut *(&raw mut LEVEL_PROCESSED)) };
    *level = LevelState::new_empty();
    *level_processed = LevelState::new_empty();
    *level = LevelState::new(x as _, y as _, z as _);
    *level_processed = LevelState::new(x as _, y as _, z as _);
    unsafe {
        *(&raw mut RNG) = None;
    }
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
    unsafe {
        *(&raw mut RNG) = None;
    }

    // Validation
    for c in level.chunks_mut() {
        for b in c.blocks_mut() {
            if matches!(
                b.get(),
                IronOre::BLOCK
                    | Block::CentralTower000
                    | Block::CentralTower001
                    | Block::CentralTower002
                    | Block::CentralTower010
                    | Block::CentralTower011
                    | Block::CentralTower012
                    | Block::CentralTower020
                    | Block::CentralTower021
                    | Block::CentralTower022
                    | Block::CentralTower100
                    | Block::CentralTower101
                    | Block::CentralTower102
                    | Block::CentralTower110
                    | Block::CentralTower111
                    | Block::CentralTower112
                    | Block::CentralTower120
                    | Block::CentralTower121
                    | Block::CentralTower122
                    | Block::CentralTower200
                    | Block::CentralTower201
                    | Block::CentralTower202
                    | Block::CentralTower210
                    | Block::CentralTower211
                    | Block::CentralTower212
                    | Block::CentralTower220
                    | Block::CentralTower221
                    | Block::CentralTower222
            ) {
                b.set(Block::Unknown);
            }
        }
    }

    struct BlockEntityValid {
        id: Uuid,
        valid: bool,
    }

    struct BlockEntitySetter {
        index: usize,
        x: usize,
        y: usize,
        z: usize,
        block: Option<Block>,
    }

    let mut be = Vec::new();
    let mut bs = Vec::new();
    for (
        &k,
        &BlockEntity {
            x, y, z, ref data, ..
        },
    ) in level.block_entities().entries()
    {
        if x / CHUNK_SIZE >= sx || y / CHUNK_SIZE >= sy || z / CHUNK_SIZE >= sz {
            be.push(BlockEntityValid {
                id: k,
                valid: false,
            });
            continue;
        }

        match data {
            BlockEntityData::IronOre(_) => bs.push(BlockEntitySetter {
                x,
                y,
                z,
                block: Some(IronOre::BLOCK),
                index: be.len(),
            }),
            BlockEntityData::Drone(_) => bs.push(BlockEntitySetter {
                x,
                y,
                z,
                block: None,
                index: be.len(),
            }),
            BlockEntityData::CentralTower(_) => bs.extend(
                [
                    Block::CentralTower000,
                    Block::CentralTower001,
                    Block::CentralTower002,
                    Block::CentralTower010,
                    Block::CentralTower011,
                    Block::CentralTower012,
                    Block::CentralTower020,
                    Block::CentralTower021,
                    Block::CentralTower022,
                    Block::CentralTower100,
                    Block::CentralTower101,
                    Block::CentralTower102,
                    Block::CentralTower110,
                    Block::CentralTower111,
                    Block::CentralTower112,
                    Block::CentralTower120,
                    Block::CentralTower121,
                    Block::CentralTower122,
                    Block::CentralTower200,
                    Block::CentralTower201,
                    Block::CentralTower202,
                    Block::CentralTower210,
                    Block::CentralTower211,
                    Block::CentralTower212,
                    Block::CentralTower220,
                    Block::CentralTower221,
                    Block::CentralTower222,
                ]
                .into_iter()
                .filter_map(|b| {
                    let (dx, dy, dz) = CentralTower::get_central_block_offset(b)?;
                    let x = x.checked_add_signed(dx)?;
                    let y = y.checked_add_signed(dy)?;
                    let z = z.checked_add_signed(dz)?;

                    if x / CHUNK_SIZE >= sx || y / CHUNK_SIZE >= sy || z / CHUNK_SIZE >= sz {
                        return None;
                    }

                    Some(BlockEntitySetter {
                        x,
                        y,
                        z,
                        block: Some(b),
                        index: be.len(),
                    })
                }),
            ),
            _ => bs.push(BlockEntitySetter {
                x,
                y,
                z,
                block: Some(Block::Unknown),
                index: be.len(),
            }),
        }

        be.push(BlockEntityValid { id: k, valid: true });
    }
    bs.sort_unstable_by(|a, b| {
        a.x.cmp(&b.x)
            .then_with(|| a.y.cmp(&b.y))
            .then_with(|| a.z.cmp(&b.z))
            .then_with(|| {
                a.block
                    .map(u16::from)
                    .cmp(&b.block.map(u16::from))
                    .reverse()
            })
            .then_with(|| be[a.index].id.cmp(&be[b.index].id))
    });

    for i in bs.windows(2) {
        let p = &i[0];
        let c = &i[1];
        if p.x == c.x && p.y == c.y && p.z == c.z {
            be[c.index].valid = false;
        }
    }

    for BlockEntitySetter {
        x,
        y,
        z,
        index,
        block,
    } in bs
    {
        if let BlockEntityValid { id, valid: false } = &be[index] {
            level.block_entities_mut().remove(id);
        } else if let Some(b) = block {
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
    unsafe {
        update(&mut *(&raw mut LEVEL), get_rng());
    }
}
