use rkyv::api::high::to_bytes_in;
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;

use level_state::{Block, BlockEntity, BlockEntityData, CHUNK_SIZE, Drone, IronOre, LevelState};
use util_wasm::write;

#[unsafe(no_mangle)]
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

    level.block_entities_mut().add(BlockEntity::new(
        1,
        1,
        1,
        BlockEntityData::Drone(Drone::new()),
    ));

    unsafe {
        write(move |buf| {
            to_bytes_in::<_, Panic>(&level, Buffer::from(buf))
                .unwrap()
                .len()
        })
    }
}
