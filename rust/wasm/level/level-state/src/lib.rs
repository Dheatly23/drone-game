use rkyv::rend::u16_le;
use rkyv::{Archive, Deserialize, Serialize};

#[derive(Debug, Archive, Serialize, Deserialize)]
pub struct LevelState {
    chunks: Vec<Chunk>,
    chunk_x: usize,
    chunk_y: usize,
    chunk_z: usize,
}

impl Default for LevelState {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl LevelState {
    pub const fn new_empty() -> Self {
        Self {
            chunks: Vec::new(),
            chunk_x: 0,
            chunk_y: 0,
            chunk_z: 0,
        }
    }

    pub fn new(x: usize, y: usize, z: usize) -> Self {
        if x == 0 || y == 0 || z == 0 {
            return Self::new_empty();
        }

        let s = match x.checked_mul(y) {
            Some(v) => v.checked_mul(z),
            None => None,
        }
        .unwrap();

        Self {
            chunks: (0..s).map(|_| Chunk::default()).collect(),
            chunk_x: x,
            chunk_y: y,
            chunk_z: z,
        }
    }

    #[inline(always)]
    pub const fn chunk_size(&self) -> (usize, usize, usize) {
        (self.chunk_x, self.chunk_y, self.chunk_z)
    }

    #[inline(always)]
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    #[inline(always)]
    pub fn chunks_mut(&mut self) -> &mut [Chunk] {
        &mut self.chunks
    }

    fn get_index(&self, x: usize, y: usize, z: usize) -> Option<usize> {
        y.checked_mul(self.chunk_z)?
            .checked_add(z)?
            .checked_mul(self.chunk_x)?
            .checked_add(x)
    }

    #[inline(always)]
    pub fn get_chunk(&self, x: usize, y: usize, z: usize) -> &Chunk {
        let i = self.get_index(x, y, z).unwrap();
        &self.chunks[i]
    }

    #[inline(always)]
    pub fn get_chunk_mut(&mut self, x: usize, y: usize, z: usize) -> &mut Chunk {
        let i = self.get_index(x, y, z).unwrap();
        &mut self.chunks[i]
    }
}

pub const CHUNK_SIZE: usize = 16;

#[derive(Debug, Archive, Serialize, Deserialize)]
pub struct Chunk {
    blocks: Box<[BlockWrapper; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE]>,
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            blocks: Box::new([const { BlockWrapper::new() }; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE]),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Archive, Serialize, Deserialize)]
#[repr(transparent)]
pub struct BlockWrapper(u16);

unsafe impl rkyv::traits::NoUndef for BlockWrapper {}

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
    ($($i:ident $(= $e:literal)?),* $(,)?) => {
        #[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
        #[repr(u16)]
        pub enum Block {
            #[default]
            Air = 0,
            $($i $(= $e)?,)*
            Unknown = u16::MAX,
        }

        impl Block {
            const fn from_u16(v: u16) -> Self {
                match v {
                    0 => Self::Air,
                    $(v if v == Self::$i as u16 => Self::$i,)*
                    _ => Self::Unknown,
                }
            }
        }
    };
}

block_def! {
    Dirt,
    Grass,
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
