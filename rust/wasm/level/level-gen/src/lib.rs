use rkyv::api::high::to_bytes_in;
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;

use level_state::{Block, IronOre, LevelState, CHUNK_SIZE};
use util_wasm::write;

#[no_mangle]
pub extern "C" fn generate() {
    let mut level = LevelState::new(16, 16, 16);

    for c in &mut level.chunks_mut()[0..16 * 16] {
        for b in &mut c.blocks_mut()[0..CHUNK_SIZE * CHUNK_SIZE] {
            b.set(Block::Grass);
        }
    }

    for z in 1..CHUNK_SIZE - 1 {
        for x in 1..CHUNK_SIZE - 1 {
            let mut v = IronOre::default();
            v.quantity = x as u64 * z as u64 * 1000;
            v.place(&mut level, x, 1, z);
        }
    }

    unsafe {
        write(move |buf| {
            to_bytes_in::<_, Panic>(&level, Buffer::from(buf))
                .unwrap()
                .len()
        })
    }
}
