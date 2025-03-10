use rand::{Rng, RngCore};
use rand_distr::Binomial;
use rkyv::rend::u16_le;
use rkyv::{Archive, Deserialize, Serialize};
use uuid::Uuid;

use crate::LevelState;
use crate::entity::{BlockEntity, BlockEntityData};
use crate::item::{Item, ItemStack};

#[derive(Debug, Eq, PartialEq, Hash, Archive, Serialize, Deserialize)]
#[repr(transparent)]
pub struct BlockWrapper(u16);

unsafe impl rkyv::traits::NoUndef for ArchivedBlockWrapper {}

impl Default for BlockWrapper {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> From<&'a BlockWrapper> for Block {
    #[inline(always)]
    fn from(v: &'a BlockWrapper) -> Self {
        v.get()
    }
}

impl<'a> From<&'a ArchivedBlockWrapper> for Block {
    #[inline(always)]
    fn from(v: &'a ArchivedBlockWrapper) -> Self {
        v.get()
    }
}

impl BlockWrapper {
    pub const fn new() -> Self {
        Self(0)
    }

    #[inline(always)]
    pub const fn get(&self) -> Block {
        Block::from_u16(self.0)
    }

    #[inline(always)]
    pub const fn set(&mut self, value: Block) {
        self.0 = value as u16;
    }
}

impl ArchivedBlockWrapper {
    pub const fn get(&self) -> Block {
        Block::from_u16(self.0.to_native())
    }

    pub const fn set(&mut self, value: Block) {
        self.0 = u16_le::from_native(value as u16);
    }
}

macro_rules! block_def {
    ($($i:ident = ($e:literal, $full:literal, $solid:literal)),* $(,)?) => {
        #[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
        #[repr(u16)]
        pub enum Block {
            #[default]
            Air = 0,
            $($i = $e,)*
            Unknown = u16::MAX,
        }

        impl Block {
            const fn from_u16(v: u16) -> Self {
                match v {
                    0 => Self::Air,
                    $($e => Self::$i,)*
                    _ => Self::Unknown,
                }
            }

            pub const fn is_full_block(&self) -> bool {
                match self {
                    Self::Air => false,
                    Self::Unknown => true,
                    $(Self::$i => $full,)*
                }
            }

            pub const fn is_solid(&self) -> bool {
                match self {
                    Self::Air => false,
                    Self::Unknown => true,
                    $(Self::$i => $solid,)*
                }
            }
        }
    };
}

block_def! {
    Dirt = (1, true, true),
    Grass = (2, true, true),
    IronOre = (0x0100, false, true),
}

impl From<u16> for Block {
    #[inline(always)]
    fn from(v: u16) -> Self {
        Self::from_u16(v)
    }
}

impl From<Block> for u16 {
    #[inline(always)]
    fn from(v: Block) -> u16 {
        v as u16
    }
}

pub struct BreakCapability<'a, R> {
    rng: &'a mut R,

    silk_touch: bool,
}

impl<'a, R: RngCore> BreakCapability<'a, R> {
    pub fn new(rng: &'a mut R) -> Self {
        Self {
            rng,
            silk_touch: false,
        }
    }

    pub fn silk_touch(mut self, value: bool) -> Self {
        self.silk_touch = value;
        self
    }
}

pub(crate) fn break_drops<R: RngCore>(
    level: &LevelState,
    x: usize,
    y: usize,
    z: usize,
    cap: BreakCapability<'_, R>,
) -> Option<(Option<Uuid>, Box<[ItemStack]>)> {
    match level.get_block(x, y, z) {
        Block::Grass if cap.silk_touch => Some((None, Box::new([ItemStack::new(Item::Grass, 1)]))),
        Block::Dirt | Block::Grass => Some((None, Box::new([ItemStack::new(Item::Dirt, 1)]))),
        Block::IronOre => {
            let Some((
                &id,
                BlockEntity {
                    data: BlockEntityData::IronOre(data),
                    ..
                },
            )) = level
                .block_entities()
                .entries()
                .find(|(_, be)| be.x == x && be.y == y && be.z == z)
            else {
                unreachable!("iron ore block entity should exist");
            };

            let n = if data.quantity == 0 {
                0
            } else {
                cap.rng.sample(Binomial::new(data.quantity, 0.8).unwrap())
            };
            let r: Box<[ItemStack]> = if n == 0 {
                Box::new([])
            } else {
                Box::new([ItemStack::new(Item::IronOre, n)])
            };

            Some((Some(id), r))
        }
        _ => None,
    }
}
