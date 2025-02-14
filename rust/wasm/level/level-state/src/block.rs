use rkyv::rend::u16_le;
use rkyv::{Archive, Deserialize, Serialize};

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
    Drone = (0x8000, false, true),
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
