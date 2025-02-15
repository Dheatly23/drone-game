use rkyv::{Archive, Deserialize, Serialize};

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Drone {
    pub command: Command,
    pub is_command_valid: bool,

    pub move_cooldown: usize,
}

impl Default for Drone {
    fn default() -> Self {
        Self::new()
    }
}

impl Drone {
    pub const fn new() -> Self {
        Self {
            command: Command::Noop,
            is_command_valid: true,

            move_cooldown: 0,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Archive, Serialize, Deserialize)]
#[non_exhaustive]
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
