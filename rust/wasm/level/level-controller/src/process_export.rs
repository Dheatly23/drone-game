use level_state::{Block, BlockEntity, BlockEntityData, LevelState};

pub fn process_to_export(dst: &mut LevelState, src: &LevelState) {
    // Censor all blocks
    for c in dst.chunks_mut() {
        for b in c.blocks_mut() {
            b.set(Block::Unknown);
        }
    }

    // Copy drone data
    dst.block_entities_mut().clone_from_filtered(
        src.block_entities(),
        |_,
         &BlockEntity {
             ref data, x, y, z, ..
         }| match data {
            BlockEntityData::Drone(_) => Some(BlockEntity::new(x, y, z, data.clone())),
            _ => None,
        },
    );
}
