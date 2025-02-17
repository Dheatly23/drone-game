use rkyv::api::high::to_bytes_in;
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;

use level_state::{Block, BlockEntity, BlockEntityData, Drone, IronOre, LevelState, CHUNK_SIZE};
use util_wasm::write;

#[no_mangle]
pub extern "C" fn generate() {
    let mut level = LevelState::new(16, 16, 16);

    for c in &mut level.chunks_mut()[0..16 * 16] {
        for b in &mut c.blocks_mut()[0..CHUNK_SIZE * CHUNK_SIZE] {
            b.set(Block::Grass);
        }
    }

    for z in 4..CHUNK_SIZE - 4 {
        for x in 4..CHUNK_SIZE - 4 {
            let mut v = IronOre::new();
            v.quantity = x as u64 * z as u64 * 1000;
            v.place(&mut level, x, 1, z);
        }
    }

    for (x, z) in (0..CHUNK_SIZE - 1).flat_map(|v| [(v, 0), (v + 1, CHUNK_SIZE - 1), (0, v + 1), (CHUNK_SIZE - 1, v)]) {
        level.block_entities_mut().add(BlockEntity::new(
            x,
            1,
            z,
            BlockEntityData::Drone(Drone::new()),
        ));
    }

    unsafe {
        write(move |buf| {
            to_bytes_in::<_, Panic>(&level, Buffer::from(buf))
                .unwrap()
                .len()
        })
    }
}
