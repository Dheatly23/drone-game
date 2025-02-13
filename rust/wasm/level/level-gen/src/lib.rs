use rkyv::api::high::to_bytes_in;
use rkyv::rancor::Panic;
use rkyv::ser::writer::Buffer;

use level_state::{Block, LevelState, CHUNK_SIZE};
use util_wasm::write;

#[no_mangle]
pub extern "C" fn generate() {
    let mut level = LevelState::new(16, 16, 16);

    for c in &mut level.chunks_mut()[0..16 * 16] {
        for b in &mut c.blocks_mut()[0..CHUNK_SIZE * CHUNK_SIZE] {
            b.set(Block::Grass);
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
