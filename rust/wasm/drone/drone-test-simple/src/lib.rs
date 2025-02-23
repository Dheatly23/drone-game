#![allow(clippy::deref_addrof)]

use rkyv::api::high::{access, to_bytes_in};
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;
use uuid::Uuid;

use level_state::{
    ArchivedBlockEntity, ArchivedBlockEntityData, ArchivedLevelState, CHUNK_SIZE, Command,
    Direction,
};
use util_wasm::{read, write};

static mut UUID: Uuid = Uuid::nil();

#[unsafe(no_mangle)]
pub extern "C" fn init(a0: u32, a1: u32, a2: u32, a3: u32) {
    unsafe {
        *(&raw mut UUID) = Uuid::from_u128(
            (a0 as u128) | ((a1 as u128) << 32) | ((a2 as u128) << 64) | ((a3 as u128) << 96),
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn tick() {
    let level = access::<ArchivedLevelState, Panic>(unsafe { read() }).unwrap();
    let Some(ArchivedBlockEntity {
        data: ArchivedBlockEntityData::Drone(_),
        x,
        z,
        ..
    }) = level.block_entities().get(unsafe { &*(&raw const UUID) })
    else {
        panic!("Drone should exist");
    };
    let x = x.to_native() as usize;
    let z = z.to_native() as usize;

    let dir = if x == 0 && z != CHUNK_SIZE - 1 {
        Direction::Forward
    } else if x == CHUNK_SIZE - 1 && z != 0 {
        Direction::Back
    } else if z == 0 {
        Direction::Right
    } else {
        Direction::Left
    };
    let cmd = Command::Move(dir);

    unsafe {
        write(|buf| {
            to_bytes_in::<_, Panic>(&cmd, Buffer::from(buf))
                .unwrap()
                .len()
        })
    }
}
