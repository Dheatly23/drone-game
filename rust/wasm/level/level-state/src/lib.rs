mod block;
mod drone;
mod entity;
mod item;

use std::iter::repeat_with;

use rand::RngCore;
use rkyv::boxed::ArchivedBox;
use rkyv::bytecheck::Verify;
use rkyv::munge::munge;
use rkyv::rancor::{Fallible, Source};
use rkyv::seal::Seal;
use rkyv::vec::ArchivedVec;
use rkyv::with::{ArchiveWith, DeserializeWith, SerializeWith};
use rkyv::{Archive, Deserialize, Place, Serialize};
use thiserror::Error;

pub use block::*;
pub use drone::*;
pub use entity::*;
pub use item::*;

#[derive(Debug, Archive, Serialize, Deserialize)]
#[rkyv(bytecheck(verify))]
pub struct LevelState {
    chunks: Vec<Chunk>,
    chunk_x: usize,
    chunk_y: usize,
    chunk_z: usize,

    entities: BlockEntities,
}

unsafe impl<C: Fallible + ?Sized> Verify<C> for ArchivedLevelState
where
    C::Error: Source,
{
    fn verify(&self, _: &mut C) -> Result<(), C::Error> {
        #[derive(Error, Debug)]
        enum LevelStateVerifyError {
            #[error("level dimension is too big (x: {x}, y: {y}, z: {z})")]
            DimensionOverflow { x: u32, y: u32, z: u32 },
            #[error("chunk vector size mismatch (expected {expect}, got {size})")]
            ChunkSizeMismatch { expect: usize, size: usize },
        }

        let Some(chunk_len) = self
            .chunk_x
            .to_native()
            .checked_mul(self.chunk_y.to_native())
            .and_then(|v| {
                Some(isize::try_from(v.checked_mul(self.chunk_z.to_native())?).ok()? as usize)
            })
        else {
            return Err(Source::new(LevelStateVerifyError::DimensionOverflow {
                x: self.chunk_x.to_native(),
                y: self.chunk_y.to_native(),
                z: self.chunk_z.to_native(),
            }));
        };
        if self.chunks.len() > chunk_len {
            return Err(Source::new(LevelStateVerifyError::ChunkSizeMismatch {
                expect: chunk_len,
                size: self.chunks.len(),
            }));
        }

        Ok(())
    }
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

            entities: BlockEntities::new(),
        }
    }

    pub fn new(x: usize, y: usize, z: usize) -> Self {
        if x == 0 || y == 0 || z == 0 {
            return Self::new_empty();
        }

        Self {
            chunks: repeat_with(Chunk::default)
                .take(x.checked_mul(y).and_then(|v| v.checked_mul(z)).unwrap())
                .collect(),
            chunk_x: x,
            chunk_y: y,
            chunk_z: z,

            ..Self::new_empty()
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
        if x >= self.chunk_x || y >= self.chunk_y || z >= self.chunk_z {
            return None;
        }
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

    #[inline(always)]
    pub fn block_entities(&self) -> &BlockEntities {
        &self.entities
    }

    #[inline(always)]
    pub fn block_entities_mut(&mut self) -> &mut BlockEntities {
        &mut self.entities
    }

    pub fn get_block(&self, x: usize, y: usize, z: usize) -> &BlockWrapper {
        self.get_chunk(x / CHUNK_SIZE, y / CHUNK_SIZE, z / CHUNK_SIZE)
            .get_block(x % CHUNK_SIZE, y % CHUNK_SIZE, z % CHUNK_SIZE)
    }

    pub fn get_block_mut(&mut self, x: usize, y: usize, z: usize) -> &mut BlockWrapper {
        self.get_chunk_mut(x / CHUNK_SIZE, y / CHUNK_SIZE, z / CHUNK_SIZE)
            .get_block_mut(x % CHUNK_SIZE, y % CHUNK_SIZE, z % CHUNK_SIZE)
    }

    pub fn break_block<R: RngCore>(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        cap: BreakCapability<'_, R>,
    ) -> Option<Box<[ItemStack]>> {
        let (id, r) = crate::block::break_drops(self, x, y, z, cap)?;
        self.get_block_mut(x, y, z).set(Block::Air);

        if let Some(id) = id {
            self.block_entities_mut().remove(&id);
        }

        Some(r)
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
        if x >= self.chunk_x.to_native() as usize
            || y >= self.chunk_y.to_native() as usize
            || z >= self.chunk_z.to_native() as usize
        {
            return None;
        }
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

    pub fn get_block(&self, x: usize, y: usize, z: usize) -> &ArchivedBlockWrapper {
        self.get_chunk(x / CHUNK_SIZE, y / CHUNK_SIZE, z / CHUNK_SIZE)
            .get_block(x % CHUNK_SIZE, y % CHUNK_SIZE, z % CHUNK_SIZE)
    }

    pub fn get_block_mut(
        this: Seal<'_, Self>,
        x: usize,
        y: usize,
        z: usize,
    ) -> &'_ mut ArchivedBlockWrapper {
        let c = Self::get_chunk_mut(this, x / CHUNK_SIZE, y / CHUNK_SIZE, z / CHUNK_SIZE);
        ArchivedChunk::get_block_mut(c, x % CHUNK_SIZE, y % CHUNK_SIZE, z % CHUNK_SIZE)
    }

    #[inline(always)]
    pub fn block_entities(&self) -> &ArchivedBlockEntities {
        &self.entities
    }

    pub fn block_entities_mut(this: Seal<'_, Self>) -> Seal<'_, ArchivedBlockEntities> {
        munge!(let Self { entities, .. } = this);
        entities
    }
}

pub const CHUNK_SIZE: usize = 16;
const TOTAL_SIZE: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

#[derive(Debug, Archive, Serialize, Deserialize)]
pub struct Chunk {
    blocks: Box<[BlockWrapper; TOTAL_SIZE]>,
    #[rkyv(with = AlwaysDirty)]
    dirty: bool,
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            blocks: Box::new([const { BlockWrapper::new() }; TOTAL_SIZE]),
            dirty: true,
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

    #[inline(always)]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    #[inline(always)]
    pub const fn mark_clean(&mut self) {
        self.dirty = false;
    }

    #[inline(always)]
    pub const fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl ArchivedChunk {
    #[inline(always)]
    pub fn blocks(&self) -> &[ArchivedBlockWrapper] {
        &self.blocks[..]
    }

    pub fn blocks_mut(this: Seal<'_, Self>) -> &mut [ArchivedBlockWrapper] {
        munge!(let Self { blocks, .. } = this);
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
        munge!(let Self { blocks, .. } = this);
        &mut ArchivedBox::get_seal(blocks).unseal()[Chunk::get_index(x, y, z).unwrap()]
    }
}

struct AlwaysDirty;

impl ArchiveWith<bool> for AlwaysDirty {
    type Archived = ();
    type Resolver = ();

    fn resolve_with(_: &bool, _: Self::Resolver, _: Place<Self::Archived>) {}
}

impl<S: Fallible + ?Sized> SerializeWith<bool, S> for AlwaysDirty {
    fn serialize_with(_: &bool, _: &mut S) -> Result<(), S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> DeserializeWith<(), bool, D> for AlwaysDirty {
    fn deserialize_with(_: &(), _: &mut D) -> Result<bool, D::Error> {
        Ok(true)
    }
}
