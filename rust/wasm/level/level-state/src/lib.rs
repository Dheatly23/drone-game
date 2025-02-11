use rkyv::boxed::ArchivedBox;
use rkyv::munge::munge;
use rkyv::rend::u16_le;
use rkyv::seal::Seal;
use rkyv::vec::ArchivedVec;
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

impl ArchivedLevelState {
    pub fn chunk_size(&self) -> (usize, usize, usize) {
        (
            self.chunk_x.to_native() as _,
            self.chunk_y.to_native() as _,
            self.chunk_z.to_native() as _,
        )
    }

    #[inline(always)]
    pub fn chunks(&self) -> &[ArchivedChunk] {
        &self.chunks
    }

    pub fn chunks_mut(this: Seal<'_, Self>) -> Seal<'_, [ArchivedChunk]> {
        munge!(let Self { chunks, .. } = this);
        ArchivedVec::as_slice_seal(chunks)
    }

    fn get_index(&self, x: usize, y: usize, z: usize) -> Option<usize> {
        y.checked_mul(self.chunk_z.to_native() as _)?
            .checked_add(z)?
            .checked_mul(self.chunk_x.to_native() as _)?
            .checked_add(x)
    }

    #[inline(always)]
    pub fn get_chunk(&self, x: usize, y: usize, z: usize) -> &ArchivedChunk {
        let i = self.get_index(x, y, z).unwrap();
        &self.chunks[i]
    }

    pub fn get_chunk_mut(
        this: Seal<'_, Self>,
        x: usize,
        y: usize,
        z: usize,
    ) -> Seal<'_, ArchivedChunk> {
        let i = this.get_index(x, y, z).unwrap();
        munge!(let Self { chunks, .. } = this);
        ArchivedVec::as_slice_seal(chunks).index(i)
    }
}

pub const CHUNK_SIZE: usize = 16;
const TOTAL_SIZE: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

#[derive(Debug, Archive, Serialize, Deserialize)]
pub struct Chunk {
    blocks: Box<[BlockWrapper; TOTAL_SIZE]>,
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            blocks: Box::new([const { BlockWrapper::new() }; TOTAL_SIZE]),
        }
    }
}

impl Chunk {
    #[inline(always)]
    pub fn blocks(&self) -> &[BlockWrapper] {
        &self.blocks[..]
    }

    #[inline(always)]
    pub fn blocks_mut(&mut self) -> &mut [BlockWrapper] {
        &mut self.blocks[..]
    }

    fn get_index(x: usize, y: usize, z: usize) -> Option<usize> {
        y.checked_mul(CHUNK_SIZE)?
            .checked_add(z)?
            .checked_mul(CHUNK_SIZE)?
            .checked_add(x)
    }

    #[inline(always)]
    pub fn get_block(&self, x: usize, y: usize, z: usize) -> &BlockWrapper {
        &self.blocks[Self::get_index(x, y, z).unwrap()]
    }

    #[inline(always)]
    pub fn get_block_mut(&mut self, x: usize, y: usize, z: usize) -> &mut BlockWrapper {
        &mut self.blocks[Self::get_index(x, y, z).unwrap()]
    }
}

impl ArchivedChunk {
    #[inline(always)]
    pub fn blocks(&self) -> &[ArchivedBlockWrapper] {
        &self.blocks[..]
    }

    pub fn blocks_mut(this: Seal<'_, Self>) -> &mut [ArchivedBlockWrapper] {
        munge!(let Self { blocks } = this);
        ArchivedBox::get_seal(blocks).unseal()
    }

    #[inline(always)]
    pub fn get_block(&self, x: usize, y: usize, z: usize) -> &ArchivedBlockWrapper {
        &self.blocks[Chunk::get_index(x, y, z).unwrap()]
    }

    pub fn get_block_mut(
        this: Seal<'_, Self>,
        x: usize,
        y: usize,
        z: usize,
    ) -> &mut ArchivedBlockWrapper {
        munge!(let Self { blocks } = this);
        &mut ArchivedBox::get_seal(blocks).unseal()[Chunk::get_index(x, y, z).unwrap()]
    }
}

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
