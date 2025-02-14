use rkyv::{Archive, Deserialize, Serialize};
use uuid::Uuid;

use crate::block::Block;
use crate::entity::{BlockEntity, BlockEntityData};
use crate::LevelState;

#[derive(Debug, Default, Clone, Archive, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Drone {
    pub command: Command,
    pub is_command_valid: bool,

    pub move_cooldown: usize,
}

impl Drone {
    pub const BLOCK: Block = Block::Drone;

    pub fn place(self, level: &mut LevelState, x: usize, y: usize, z: usize) -> Uuid {
        BlockEntity::new(x, y, z, BlockEntityData::Drone(self)).place(level, Self::BLOCK)
    }
}

#[derive(Debug, Default, Clone, Copy, Archive, Serialize, Deserialize)]
pub enum Command {
    #[default]
    Noop,
    Move(Direction),
}

#[derive(Debug, Clone, Copy, Archive, Serialize, Deserialize)]
pub enum Direction {
    // Cardinal
    Up,
    Down,
    Left,
    Right,
    Forward,
    Back,

    // Diagonal
    ForwardLeft,
    ForwardRight,
    BackLeft,
    BackRight,
    UpLeft,
    UpRight,
    UpForward,
    UpBack,
    DownLeft,
    DownRight,
    DownForward,
    DownBack,
}
