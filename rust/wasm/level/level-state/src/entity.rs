use std::hash::BuildHasher;

use hashbrown::hash_map::{Entry, HashMap};
use hashbrown::hash_set::HashSet;
use rkyv::hash::FxHasher64;
use rkyv::with::{AsBox, Skip};
use rkyv::{Archive, Deserialize, Serialize};
use uuid::Uuid;

use crate::block::Block;
use crate::drone::{Command, Drone, DroneCapability};
use crate::item::ItemSlot;
use crate::{CHUNK_SIZE, LevelState};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BlockEntityHasher;

impl BuildHasher for BlockEntityHasher {
    type Hasher = FxHasher64;

    fn build_hasher(&self) -> Self::Hasher {
        FxHasher64::default()
    }
}

#[derive(Debug, Default, Archive, Serialize, Deserialize)]
pub struct BlockEntities {
    data: HashMap<Uuid, BlockEntity, BlockEntityHasher>,
    #[rkyv(with = Skip)]
    grave: HashSet<Uuid, BlockEntityHasher>,
}

fn filter_clone((&k, v): (&Uuid, &BlockEntity)) -> Option<(Uuid, BlockEntity)> {
    let BlockEntity {
        x, y, z, ref data, ..
    } = *v;
    Some((k, BlockEntity::new(x, y, z, data.clone())))
}

impl Clone for BlockEntities {
    fn clone(&self) -> Self {
        Self {
            data: self.data.iter().filter_map(filter_clone).collect(),
            ..Self::new()
        }
    }

    fn clone_from(&mut self, src: &Self) {
        self.data.clear();
        self.data.extend(src.data.iter().filter_map(filter_clone));
    }
}

impl BlockEntities {
    pub const fn new() -> Self {
        Self {
            data: HashMap::with_hasher(BlockEntityHasher),
            grave: HashSet::with_hasher(BlockEntityHasher),
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn add(&mut self, entity: BlockEntity) -> Uuid {
        let entry = loop {
            if let Entry::Vacant(v) = self.data.entry(Uuid::new_v4()) {
                if !self.grave.contains(v.key()) {
                    break v;
                }
            }
        };
        *entry.insert_entry(entity).key()
    }

    #[inline(always)]
    pub fn remove(&mut self, uuid: &Uuid) -> Option<BlockEntity> {
        let ret = self.data.remove(uuid);
        if ret.is_some() {
            self.grave.insert(*uuid);
        }
        ret
    }

    pub fn remove_if<F: FnMut(&Uuid, &mut BlockEntity) -> bool>(&mut self, f: F) {
        self.grave.extend(self.data.extract_if(f).map(|(k, _)| k));
    }

    #[inline(always)]
    pub fn clear_grave(&mut self) {
        self.grave.clear();
    }

    #[inline(always)]
    pub fn pop_removed(&mut self) -> impl '_ + Iterator<Item = Uuid> {
        self.grave.drain()
    }

    #[inline(always)]
    pub fn get(&self, uuid: &Uuid) -> Option<&BlockEntity> {
        self.data.get(uuid)
    }

    #[inline(always)]
    pub fn get_mut(&mut self, uuid: &Uuid) -> Option<&mut BlockEntity> {
        self.data.get_mut(uuid)
    }

    #[inline(always)]
    pub fn entries(&self) -> impl Iterator<Item = (&'_ Uuid, &'_ BlockEntity)> {
        self.data.iter()
    }

    #[inline(always)]
    pub fn entries_mut(&mut self) -> impl Iterator<Item = (&'_ Uuid, &'_ mut BlockEntity)> {
        self.data.iter_mut()
    }

    #[inline(always)]
    pub fn keys(&self) -> impl Iterator<Item = &'_ Uuid> {
        self.data.keys()
    }

    pub fn clone_from_filtered(
        &mut self,
        src: &Self,
        mut f: impl FnMut(&Uuid, &BlockEntity) -> Option<BlockEntity>,
    ) {
        self.data.clear();
        self.data
            .extend(src.entries().filter_map(move |(k, v)| Some((*k, f(k, v)?))));
    }
}

impl ArchivedBlockEntities {
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[inline(always)]
    pub fn get(&self, uuid: &Uuid) -> Option<&ArchivedBlockEntity> {
        self.data.get(uuid)
    }

    #[inline(always)]
    pub fn entries(&self) -> impl Iterator<Item = (&'_ Uuid, &'_ ArchivedBlockEntity)> {
        self.data.iter()
    }

    #[inline(always)]
    pub fn keys(&self) -> impl Iterator<Item = &'_ Uuid> {
        self.data.keys()
    }
}

#[derive(Debug, Archive, Serialize, Deserialize)]
pub struct BlockEntity {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub data: BlockEntityData,

    #[rkyv(with = crate::AlwaysDirty)]
    dirty: bool,
}

impl BlockEntity {
    #[inline(always)]
    pub const fn new(x: usize, y: usize, z: usize, data: BlockEntityData) -> Self {
        Self {
            x,
            y,
            z,
            data,

            dirty: true,
        }
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

    pub(crate) fn place(self, level: &mut LevelState, block: Block) -> Uuid {
        let Self { x, y, z, .. } = self;
        level.get_block_mut(x, y, z).set(block);
        level.block_entities_mut().add(self)
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub enum BlockEntityData {
    IronOre(IronOre),
    Drone(#[rkyv(with = AsBox)] Drone),
    CentralTower(#[rkyv(with = AsBox)] CentralTower),
}

#[derive(Debug, Clone, Copy, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub struct IronOre {
    pub quantity: u64,
}

impl Default for IronOre {
    fn default() -> Self {
        Self::new()
    }
}

impl IronOre {
    pub const BLOCK: Block = Block::IronOre;

    pub const fn new() -> Self {
        Self { quantity: 0 }
    }

    pub fn place(self, level: &mut LevelState, x: usize, y: usize, z: usize) -> Uuid {
        BlockEntity::new(x, y, z, BlockEntityData::IronOre(self)).place(level, Self::BLOCK)
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(attr(non_exhaustive))]
#[non_exhaustive]
pub struct CentralTower {
    pub command: Command,
    pub is_command_valid: bool,

    pub capabilities: DroneCapability,

    pub inventory: Box<[ItemSlot; 9 * 3]>,

    pub exec: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Default for CentralTower {
    fn default() -> Self {
        Self::new()
    }
}

impl CentralTower {
    pub const BLOCK: Block = Block::CentralTower;

    pub fn new() -> Self {
        Self {
            command: Command::Noop,
            is_command_valid: true,

            capabilities: DroneCapability::new(),

            inventory: Box::default(),

            exec: String::new(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    pub fn clone_censored(&self) -> Self {
        Self {
            command: Command::Noop,
            is_command_valid: self.is_command_valid,

            capabilities: self.capabilities.clone(),

            inventory: self.inventory.clone(),

            exec: String::new(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    pub fn place(self, level: &mut LevelState, x: usize, y: usize, z: usize) -> Uuid {
        let ret = BlockEntity::new(x, y, z, BlockEntityData::CentralTower(self))
            .place(level, Self::BLOCK);

        let (sx, sy, sz) = level.chunk_size();
        for x in
            (-1isize..2).filter_map(|d| x.checked_add_signed(d).filter(|v| v / CHUNK_SIZE < sx))
        {
            for z in
                (-1isize..2).filter_map(|d| z.checked_add_signed(d).filter(|v| v / CHUNK_SIZE < sz))
            {
                for y in (0isize..3)
                    .filter_map(|d| y.checked_add_signed(d).filter(|v| v / CHUNK_SIZE < sy))
                {
                    level.get_block_mut(x, y, z).set(Self::BLOCK);
                }
            }
        }

        ret
    }
}
