use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::mem::{replace, swap};

use enumflags2::{BitFlag, BitFlags, bitflags, make_bitflags};
use rkyv::bytecheck::{CheckBytes, Verify};
use rkyv::munge::munge;
use rkyv::primitive::ArchivedU16;
use rkyv::rancor::{Fallible, Source};
use rkyv::traits::{NoUndef, Portable};
use rkyv::with::{ArchiveWith, DeserializeWith, SerializeWith};
use rkyv::{Archive, Deserialize, Place, Serialize};
use thiserror::Error;

use crate::block::Block;

macro_rules! item_def {
    ($($i:ident = ($e:literal, $n:literal, $place:expr)),* $(,)?) => {
        #[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
        #[repr(u16)]
        pub enum Item {
            #[default]
            Air = 0,
            $($i = $e,)*
            Unknown = u16::MAX,
        }

        impl Item {
            const fn from_u16(v: u16) -> Self {
                match v {
                    0 => Self::Air,
                    $($e => Self::$i,)*
                    _ => Self::Unknown,
                }
            }

            pub const fn stack_count(&self) -> u8 {
                match self {
                    Self::Air => 0,
                    Self::Unknown => 64,
                    $(Self::$i => $n,)*
                }
            }

            pub const fn place_block(&self) -> Option<Block> {
                match self {
                    Self::Air => None,
                    Self::Unknown => None,
                    $(Self::$i => $place,)*
                }
            }
        }
    };
}

item_def! {
    Dirt = (1, 64, Some(Block::Dirt)),
    Grass = (2, 64, Some(Block::Grass)),
    IronOre = (0x0100, 64, None),
}

impl From<u16> for Item {
    #[inline(always)]
    fn from(v: u16) -> Self {
        Self::from_u16(v)
    }
}

impl From<Item> for u16 {
    #[inline(always)]
    fn from(v: Item) -> u16 {
        v as u16
    }
}

impl Archive for Item {
    type Archived = ArchivedU16;
    type Resolver = <u16 as Archive>::Resolver;

    fn resolve(&self, resolver: Self::Resolver, out: Place<Self::Archived>) {
        (*self as u16).resolve(resolver, out);
    }
}

impl<D: Fallible + ?Sized> Deserialize<Item, D> for <Item as Archive>::Archived {
    fn deserialize(&self, deserializer: &mut D) -> Result<Item, D::Error> {
        Deserialize::<u16, D>::deserialize(self, deserializer).map(Item::from_u16)
    }
}

impl<S: Fallible + ?Sized> Serialize<S> for Item {
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        (*self as u16).serialize(serializer)
    }
}

pub(crate) struct BitFlagsDef;

#[repr(transparent)]
pub struct ArchivedBitFlagsDef<T>
where
    T: BitFlag,
    T::Numeric: Archive,
{
    inner: <T::Numeric as Archive>::Archived,
}

impl<T> Debug for ArchivedBitFlagsDef<T>
where
    T: BitFlag,
    T::Numeric: Archive,
    <T::Numeric as Archive>::Archived: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ArchivedBitFlagsDef")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<T> Clone for ArchivedBitFlagsDef<T>
where
    T: BitFlag,
    T::Numeric: Archive,
    <T::Numeric as Archive>::Archived: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Copy for ArchivedBitFlagsDef<T>
where
    T: BitFlag,
    T::Numeric: Archive,
    <T::Numeric as Archive>::Archived: Copy,
{
}

unsafe impl<T> Portable for ArchivedBitFlagsDef<T>
where
    T: BitFlag,
    T::Numeric: Archive,
    <T::Numeric as Archive>::Archived: Portable,
{
}

unsafe impl<T> NoUndef for ArchivedBitFlagsDef<T>
where
    T: BitFlag,
    T::Numeric: Archive,
    <T::Numeric as Archive>::Archived: NoUndef,
{
}

unsafe impl<T, C: Fallible + ?Sized> CheckBytes<C> for ArchivedBitFlagsDef<T>
where
    T: BitFlag,
    T::Numeric: Archive,
    <T::Numeric as Archive>::Archived: CheckBytes<C>,
{
    unsafe fn check_bytes(value: *const Self, context: &mut C) -> Result<(), C::Error> {
        unsafe {
            CheckBytes::check_bytes(value.cast::<<T::Numeric as Archive>::Archived>(), context)
        }
    }
}

impl<T> ArchivedBitFlagsDef<T>
where
    T: BitFlag,
    T::Numeric: Archive,
{
    pub fn into_bit_flags(self) -> BitFlags<T>
    where
        <T::Numeric as Archive>::Archived: Into<T::Numeric>,
    {
        BitFlags::from_bits_truncate(self.inner.into())
    }
}

#[repr(transparent)]
pub(crate) struct BitFlagsDefResolver<T>
where
    T: BitFlag,
    T::Numeric: Archive,
{
    inner: <T::Numeric as Archive>::Resolver,
}

impl<T> ArchiveWith<BitFlags<T>> for BitFlagsDef
where
    T: BitFlag,
    T::Numeric: Archive,
{
    type Archived = ArchivedBitFlagsDef<T>;
    type Resolver = BitFlagsDefResolver<T>;

    fn resolve_with(field: &BitFlags<T>, resolver: Self::Resolver, out: Place<Self::Archived>) {
        munge!(let ArchivedBitFlagsDef { inner } = out);
        field.bits().resolve(resolver.inner, inner);
    }
}

impl<D: Fallible + ?Sized, T> DeserializeWith<ArchivedBitFlagsDef<T>, BitFlags<T>, D>
    for BitFlagsDef
where
    T: BitFlag,
    T::Numeric: Archive,
    <T::Numeric as Archive>::Archived: Deserialize<T::Numeric, D>,
{
    fn deserialize_with(
        field: &ArchivedBitFlagsDef<T>,
        deserializer: &mut D,
    ) -> Result<BitFlags<T>, D::Error> {
        field
            .inner
            .deserialize(deserializer)
            .map(BitFlags::from_bits_truncate)
    }
}

impl<S: Fallible + ?Sized, T> SerializeWith<BitFlags<T>, S> for BitFlagsDef
where
    T: BitFlag,
    T::Numeric: Serialize<S>,
{
    fn serialize_with(
        field: &BitFlags<T>,
        serializer: &mut S,
    ) -> Result<BitFlagsDefResolver<T>, S::Error> {
        Ok(BitFlagsDefResolver {
            inner: field.bits().serialize(serializer)?,
        })
    }
}

#[bitflags]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum SlotFlags {
    Insert = 0b0000_0001,
    Extract = 0b0000_0010,
    Typed = 0b1000_0000,
}

#[derive(Debug, Clone, Eq, PartialEq, Archive, Serialize, Deserialize)]
#[rkyv(bytecheck(verify))]
pub struct ItemSlot {
    item: Item,
    count: u8,
    #[rkyv(with = BitFlagsDef)]
    pub slot_flags: BitFlags<SlotFlags>,
}

unsafe impl<C: Fallible + ?Sized> Verify<C> for ArchivedItemSlot
where
    C::Error: Source,
{
    fn verify(&self, _: &mut C) -> Result<(), C::Error> {
        #[derive(Error, Debug)]
        enum ItemSlotVerifyError {
            #[error("item count overflow (maximum is {max}, got {count})")]
            CountOverflow { count: u8, max: u8 },
        }

        let max_count = self.item().stack_count();
        if self.count > max_count {
            return Err(Source::new(ItemSlotVerifyError::CountOverflow {
                count: self.count,
                max: max_count,
            }));
        }

        Ok(())
    }
}

impl Default for ItemSlot {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl PartialEq<Item> for ItemSlot {
    #[inline(always)]
    fn eq(&self, other: &Item) -> bool {
        self.item.eq(other)
    }
}

impl ItemSlot {
    pub const fn new_empty() -> Self {
        Self {
            item: Item::Air,
            count: 0,
            slot_flags: BitFlags::EMPTY,
        }
    }

    pub fn with_item(item: Item, count: u8) -> Self {
        Self {
            item,
            count: count.min(item.stack_count()),
            ..Self::new_empty()
        }
    }

    #[inline(always)]
    pub const fn item(&self) -> Item {
        self.item
    }

    pub fn set_item(&mut self, item: Item) {
        self.item = item;
        self.count = self.count.min(self.item.stack_count());
    }

    #[inline(always)]
    pub const fn count(&self) -> u8 {
        self.count
    }

    pub fn set_count(&mut self, n: u8) {
        if n == 0 {
            if !self.slot_flags.contains(SlotFlags::Typed) {
                self.item = Item::Air;
            }
            self.count = 0;
            return;
        }

        self.count = n.min(self.item.stack_count());
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn is_full(&self) -> bool {
        !matches!(self.item, Item::Air) && self.count == self.item.stack_count()
    }

    pub fn add_item(&mut self, n: u8) -> u8 {
        let t = self.count + n;
        let m = self.item.stack_count();
        if t <= m {
            self.count = t;
            0
        } else {
            self.count = m;
            t - m
        }
    }

    pub fn remove_item(&mut self, n: u8) -> u8 {
        if self.count >= n {
            self.count -= n;
            n
        } else {
            replace(&mut self.count, 0)
        }
    }

    pub fn swap_slot(&mut self, other: &mut Self) {
        if !self
            .slot_flags
            .contains(make_bitflags!(SlotFlags::{Extract | Insert}))
            || !other
                .slot_flags
                .contains(make_bitflags!(SlotFlags::{Extract | Insert}))
        {
            return;
        }

        match (
            self.slot_flags.contains(SlotFlags::Typed),
            &mut self.item,
            other.slot_flags.contains(SlotFlags::Typed),
            &mut other.item,
        ) {
            (_, Item::Air, _, Item::Air) => return,
            (false, a, false, b) => swap(a, b),
            (true, i, _, t @ Item::Air) | (_, t @ Item::Air, true, i) => *t = *i,
            (true, a, _, b) | (_, a, true, b) if a != b => return,
            _ => (),
        }

        swap(&mut self.count, &mut other.count);
    }

    pub fn transfer_slot(&mut self, src: &mut Self, max: Option<&mut u64>) {
        if matches!(max, Some(0))
            || src.is_empty()
            || !src.slot_flags.contains(SlotFlags::Extract)
            || !self.slot_flags.contains(SlotFlags::Insert)
        {
            return;
        }

        match (src.item, self.item) {
            (Item::Air, _) => (),
            (_, Item::Air) => {
                self.item = src.item;
                if let Some(max) = max {
                    if src.count as u64 > *max {
                        let n = replace(max, 0) as u8;
                        self.count = n;
                        src.count -= n;
                        return;
                    } else {
                        *max -= src.count as u64;
                    }
                }

                if !src.slot_flags.contains(SlotFlags::Typed) {
                    src.item = Item::Air;
                }
                self.count = replace(&mut src.count, 0);
            }
            (a, b) if a != b => (),
            (_, _) => {
                if let Some(max) = max {
                    if src.count as u64 > *max {
                        let n = *max as u8;
                        let t = self.add_item(n);
                        src.count = src.count - n + t;
                        *max = t as u64;
                    } else {
                        let prev = src.count;
                        src.count = self.add_item(src.count);
                        *max -= (prev - src.count) as u64;
                    }
                } else {
                    src.count = self.add_item(src.count);
                }

                if src.count == 0 && !src.slot_flags.contains(SlotFlags::Typed) {
                    src.item = Item::Air;
                }
            }
        }
    }

    pub fn push_inventory(&mut self, inventory: &mut [Self], mut max: Option<&mut u64>) {
        if matches!(max, Some(0))
            || self.is_empty()
            || !self.slot_flags.contains(SlotFlags::Extract)
        {
            return;
        }

        // Insert into filled slots
        for slot in inventory.iter_mut() {
            if slot.is_empty() || !slot.slot_flags.contains(SlotFlags::Insert) {
                continue;
            }

            slot.transfer_slot(self, max.as_deref_mut());
            if self.is_empty() || matches!(max, Some(0)) {
                return;
            }
        }

        // Insert into empty slots
        for slot in inventory.iter_mut() {
            if !slot.is_empty() || !slot.slot_flags.contains(SlotFlags::Insert) {
                continue;
            }

            slot.transfer_slot(self, max.as_deref_mut());
            if self.is_empty() || matches!(max, Some(0)) {
                return;
            }
        }
    }

    pub fn pull_inventory(&mut self, inventory: &mut [Self], mut max: Option<&mut u64>) {
        if matches!(max, Some(0)) || self.is_full() || !self.slot_flags.contains(SlotFlags::Insert)
        {
            return;
        }

        for slot in inventory.iter_mut() {
            if slot.is_empty() || !slot.slot_flags.contains(SlotFlags::Extract) {
                continue;
            }

            self.transfer_slot(slot, max.as_deref_mut());
            if self.is_full() || matches!(max, Some(0)) {
                return;
            }
        }
    }

    pub fn transfer_inventory(
        dst: &mut [Self],
        src: &mut [Self],
        mut max: Option<&mut u64>,
    ) -> bool {
        if matches!(max, Some(0))
            || !src
                .iter()
                .any(|slot| !slot.is_empty() && slot.slot_flags.contains(SlotFlags::Extract))
            || !dst
                .iter()
                .any(|slot| !slot.is_full() && slot.slot_flags.contains(SlotFlags::Insert))
        {
            return false;
        }

        // Insert into filled slots
        for src in src.iter_mut() {
            if src.is_empty() || !src.slot_flags.contains(SlotFlags::Extract) {
                continue;
            }

            for dst in dst.iter_mut() {
                if dst.is_empty() || !dst.slot_flags.contains(SlotFlags::Insert) {
                    continue;
                }

                dst.transfer_slot(src, max.as_deref_mut());
                if matches!(max, Some(0)) {
                    return false;
                } else if src.is_empty() {
                    break;
                }
            }
        }

        // Insert into empty slots
        let mut cont = false;
        'outer: for src in src.iter_mut() {
            if src.is_empty() || !src.slot_flags.contains(SlotFlags::Extract) {
                continue;
            }

            for dst in dst.iter_mut() {
                if !dst.is_empty() || !dst.slot_flags.contains(SlotFlags::Insert) {
                    continue;
                }

                dst.transfer_slot(src, max.as_deref_mut());
                if matches!(max, Some(0)) {
                    return false;
                } else if src.is_empty() {
                    continue 'outer;
                }
            }

            cont = true;
        }

        cont
    }
}

impl ArchivedItemSlot {
    #[inline(always)]
    pub fn item(&self) -> Item {
        self.item.to_native().into()
    }

    #[inline(always)]
    pub fn count(&self) -> u8 {
        self.count
    }

    #[inline(always)]
    pub fn slot_flags(&self) -> BitFlags<SlotFlags> {
        self.slot_flags.into_bit_flags()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn is_full(&self) -> bool {
        let item = self.item();
        !matches!(item, Item::Air) && self.count == item.stack_count()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Archive, Deserialize, Serialize)]
pub struct ItemStack {
    pub item: Item,
    pub count: u64,
}

impl ItemStack {
    pub const fn new(item: Item, count: u64) -> Self {
        Self { item, count }
    }

    pub fn put_slot(&mut self, dst: &mut ItemSlot) {
        if self.count == 0 || !dst.slot_flags.contains(SlotFlags::Insert) {
            return;
        }

        if dst.item == Item::Air {
            dst.item = self.item;
        } else if dst.item != self.item {
            return;
        }
        let n = u8::try_from(self.count)
            .unwrap_or(255)
            .min(self.item.stack_count());
        dst.count = n;
        self.count -= n as u64;
    }

    pub fn put_inventory(src: &mut [Self], dst: &mut [ItemSlot]) -> bool {
        let mut cont = false;
        'outer: for src in src.iter_mut() {
            if src.count == 0 {
                continue;
            }

            for dst in dst.iter_mut() {
                if dst.is_empty() || !dst.slot_flags.contains(SlotFlags::Insert) {
                    continue;
                }

                src.put_slot(dst);

                if src.count == 0 {
                    continue 'outer;
                }
            }

            cont = true;
        }

        if !cont {
            return false;
        }

        cont = false;
        'outer: for src in src.iter_mut() {
            if src.count == 0 {
                continue;
            }

            for dst in dst.iter_mut() {
                if !dst.is_empty() || !dst.slot_flags.contains(SlotFlags::Insert) {
                    continue;
                }

                src.put_slot(dst);

                if src.count == 0 {
                    continue 'outer;
                }
            }

            cont = true;
        }

        cont
    }
}
