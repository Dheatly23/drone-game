use enumflags2::{BitFlags, bitflags};
use rkyv::with::{Niche, Skip};
use rkyv::{Archive, Deserialize, Serialize};

use crate::item::{BitFlagsDef, ItemSlot};

#[derive(Debug, Default, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub struct ExecutionContext {
    pub executable: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl ExecutionContext {
    pub fn executable(mut self, value: String) -> Self {
        self.executable = value;
        self
    }

    pub fn args(mut self, value: Vec<String>) -> Self {
        self.args = value;
        self
    }

    pub fn env(mut self, value: Vec<(String, String)>) -> Self {
        self.env = value;
        self
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub struct Drone {
    #[rkyv(with = Skip)]
    pub command: Command,
    pub is_command_valid: bool,
    pub move_cooldown: usize,

    pub capabilities: DroneCapability,

    pub inventory: [ItemSlot; 9 * 3],

    #[rkyv(with = Skip)]
    pub exec: Option<ExecutionContext>,
}

impl Default for Drone {
    fn default() -> Self {
        Self::new()
    }
}

impl Drone {
    pub fn new() -> Self {
        Self {
            command: Command::Noop,
            is_command_valid: true,
            move_cooldown: 0,

            capabilities: DroneCapability::new(),

            inventory: Default::default(),

            exec: None,
        }
    }

    pub fn clone_censored(&self) -> Self {
        Self {
            command: Command::Noop,
            is_command_valid: self.is_command_valid,
            move_cooldown: self.move_cooldown,

            capabilities: self.capabilities.clone(),

            inventory: self.inventory.clone(),

            exec: None,
        }
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub struct DroneCapability {
    #[rkyv(with = BitFlagsDef)]
    pub flags: BitFlags<DroneCapabilityFlags>,

    #[rkyv(with = Niche)]
    pub ext_inventory: Option<Box<[ItemSlot; 9 * 3]>>,
}

#[bitflags]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
#[repr(u64)]
pub enum DroneCapabilityFlags {
    Moving,
    Flying,
    Breaker,
    SilkTouch,
    ExtendedInventory,
    DroneSummon,
}

impl Default for DroneCapability {
    fn default() -> Self {
        Self::new()
    }
}

impl DroneCapability {
    pub const fn new() -> Self {
        Self {
            flags: BitFlags::EMPTY,

            ext_inventory: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub enum Command {
    #[default]
    Noop,
    Move(Direction),

    Place(InventorySlot, Direction),
    Break(Direction),
    Mine(Direction),

    PullInventory {
        direction: Direction,
        src_inv: InventoryType,
        src_slot: usize,
        dst_inv: InventoryType,
        dst_slot: usize,
        count: u8,
    },
    PushInventory {
        direction: Direction,
        src_inv: InventoryType,
        src_slot: usize,
        dst_inv: InventoryType,
        dst_slot: usize,
        count: u8,
    },
    InventoryOps(Vec<InventoryOp>),

    Summon(
        Direction,
        ExecutionContext,
        #[rkyv(with = BitFlagsDef)] BitFlags<DroneCapabilityFlags>,
    ),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub enum InventoryType {
    #[default]
    Inventory,
    ExtInventory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct InventorySlot {
    pub inventory: InventoryType,
    pub slot: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub enum InventoryOp {
    Swap {
        src: InventorySlot,
        dst: InventorySlot,
    },
    Transfer {
        src: InventorySlot,
        dst: InventorySlot,
        count: u8,
    },
    Pull {
        src: InventoryType,
        dst: InventorySlot,
        count: u64,
    },
    Push {
        dst: InventoryType,
        src: InventorySlot,
        count: u64,
    },
}
