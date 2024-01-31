use std::cmp::Ordering;
use std::num::NonZeroU8;

use ndarray::{Array3, Dimension};

use super::blocks::{block_drops, block_place, block_type, is_valid, BlockType};
use super::{Mesh, State, OCCUPIED_FLAG};

const INVENTORY_SIZE: usize = 9;

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Drone {
    pub x: usize,
    pub y: usize,
    pub z: usize,

    pub command: Command,
    pub inventory: [Inventory; INVENTORY_SIZE],
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(u8)]
pub enum Command {
    #[default]
    Noop,
    Move(Dir),
    BreakBlock(Dir),
    PlaceBlock(Dir, u8),
    SendItem(Dir, u8),
    RecvItem(Dir, u8),
    Restack,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Dir {
    #[default]
    Noop,
    Up,
    Down,
    Left,
    Right,
    Front,
    Back,
}

impl Dir {
    pub fn move_coord(
        &self,
        size: &(usize, usize, usize),
        coord: (usize, usize, usize),
    ) -> Option<(usize, usize, usize)> {
        Some(match self {
            Self::Noop => coord,
            Self::Up if coord.1 + 1 < size.1 => (coord.0, coord.1 + 1, coord.2),
            Self::Down if coord.1 > 0 => (coord.0, coord.1 - 1, coord.2),
            Self::Left if coord.0 + 1 < size.0 => (coord.0 + 1, coord.1, coord.2),
            Self::Right if coord.0 > 0 => (coord.0 - 1, coord.1, coord.2),
            Self::Back if coord.2 + 1 < size.2 => (coord.0, coord.1, coord.2 + 1),
            Self::Front if coord.2 > 0 => (coord.0, coord.1, coord.2 - 1),
            _ => return None,
        })
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Inventory {
    pub item_id: Option<NonZeroU8>,
    pub count: u8,
}

impl Inventory {
    pub const MAX_STACK: u8 = 64;

    pub const fn new(item_id: Option<NonZeroU8>, count: u8) -> Self {
        Self {
            count: if item_id.is_none() { 0 } else { count },
            item_id,
        }
    }

    pub fn try_put_one(this: &mut [Self], src: &mut Self) {
        for d in &mut *this {
            if d.item_id != src.item_id {
                continue;
            }
            let n = src.count.min(Self::MAX_STACK - d.count);
            d.count += n;
            src.count -= n;
            if src.count == 0 {
                src.item_id = None;
                return;
            }
        }

        for d in this {
            if d.item_id.is_some() {
                continue;
            }
            (d.item_id, d.count) = (src.item_id, src.count);
            (src.item_id, src.count) = (None, 0);
            return;
        }
    }

    pub fn try_put_many(this: &mut [Self], mut src: &mut [Self]) -> bool {
        if src.iter().all(|v| v.item_id.is_none()) {
            return true;
        }

        for s in &mut *src {
            if s.count == 0 {
                s.item_id = None;
            }
        }
        src.sort_unstable_by_key(|v| v.item_id);
        let i = src.partition_point(|v| v.item_id.is_none());
        src = &mut src[i..];
        if src.is_empty() {
            return true;
        }

        for d in this.iter_mut() {
            if d.item_id.is_none() {
                continue;
            }

            let i = src.partition_point(|v| v.item_id < d.item_id);
            let mut n = Self::MAX_STACK - d.count;
            let mut j = 0;
            for s in &mut src[i..] {
                if s.item_id != d.item_id {
                    continue;
                }
                let n_ = s.count.min(n);
                d.count += n_;
                n -= n_;
                s.count -= n_;
                if s.count == 0 {
                    s.item_id = None;
                    j += 1;
                }
                if n == 0 {
                    break;
                }
            }

            if j > 0 {
                src[..i + j].rotate_right(j);
                src = &mut src[j..];
                if src.is_empty() {
                    return true;
                }
            }
        }

        for d in this {
            if d.item_id.is_some() {
                continue;
            }
            d.count = 0;
            let mut n = Self::MAX_STACK;
            while let Some(s) = src.first_mut() {
                d.item_id = s.item_id;
                let n_ = s.count.min(n);
                d.count += n_;
                n -= n_;
                s.count -= n_;
                if s.count == 0 {
                    s.item_id = None;
                    src = &mut src[1..];
                }
                if n == 0 {
                    break;
                }
            }
            if src.is_empty() {
                return true;
            }
        }

        src.is_empty()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MoveIndex {
    x: usize,
    y: usize,
    z: usize,
    i: usize,
}

impl PartialEq for MoveIndex {
    fn eq(&self, Self { x, y, z, .. }: &Self) -> bool {
        self.x.eq(x) && self.y.eq(y) && self.z.eq(z)
    }
}

impl Eq for MoveIndex {}

impl Ord for MoveIndex {
    fn cmp(&self, Self { x, y, z, .. }: &Self) -> Ordering {
        match self.x.cmp(x) {
            Ordering::Equal => match self.z.cmp(z) {
                Ordering::Equal => self.y.cmp(y),
                v => v,
            },
            v => v,
        }
    }
}

impl PartialOrd for MoveIndex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl MoveIndex {
    fn cmp_coord(&self, (x, y, z): &(usize, usize, usize)) -> Ordering {
        match self.x.cmp(x) {
            Ordering::Equal => match self.z.cmp(z) {
                Ordering::Equal => self.y.cmp(y),
                v => v,
            },
            v => v,
        }
    }
}

fn mark_dirty(
    mesh: &mut Array3<Mesh>,
    chunks_size: usize,
    (mut x, mut y, mut z): (usize, usize, usize),
) {
    x /= chunks_size;
    y /= chunks_size;
    z /= chunks_size;
    if let Some(m) = mesh.get_mut((x, y, z)) {
        m.dirty = true;
    }
}

pub fn execute_commands(state: &mut State) {
    let size = state.data.raw_dim().into_pattern();

    let mut has_move = false;
    for (((i, d), m), r) in state
        .drones
        .iter_mut()
        .enumerate()
        .zip(&mut state.move_index)
        .zip(&mut state.rev_index)
    {
        *r = MoveIndex {
            x: d.x,
            y: d.y,
            z: d.z,
            i,
        };
        *m = *r;
        let mut c = None;
        if let Command::Move(dir) = d.command {
            if dir != Dir::Noop {
                c = dir
                    .move_coord(&size, (d.x, d.y, d.z))
                    .filter(|&i| block_type((state.data[i] & 0xff) as _) != BlockType::Full)
            }
        }

        if let Some(c) = c {
            has_move = true;
            (m.x, m.y, m.z) = c;
        } else if matches!(d.command, Command::Move(_)) {
            d.command = Command::Noop;
        }
    }

    state.rev_index.sort_unstable();

    for d in &mut state.drones {
        let Command::BreakBlock(dir) = d.command else {
            continue;
        };
        d.command = Command::Noop;

        let Some(c) = dir.move_coord(&size, (d.x, d.y, d.z)) else {
            continue;
        };
        let b = &mut state.data[c];
        let t = (*b & 0xff) as u8;
        if (t == 0) || !is_valid(t) {
            continue;
        }
        if block_drops(t, &mut state.rng, |src| {
            Inventory::try_put_many(&mut d.inventory, src);
            true
        }) {
            *b &= !0xff;
            mark_dirty(&mut state.mesh, state.chunks_size, c);
        }
    }

    for d in &mut state.drones {
        let Command::PlaceBlock(dir, slot) = d.command else {
            continue;
        };
        d.command = Command::Noop;

        let Some(slot) = d.inventory.get_mut(slot as usize) else {
            continue;
        };
        if slot.count == 0 {
            continue;
        }
        let Some(i) = slot.item_id else {
            continue;
        };
        let Some(c) = dir.move_coord(&size, (d.x, d.y, d.z)) else {
            continue;
        };
        let b = &mut state.data[c];
        let t = (*b & 0xff) as u8;
        if t != 0 {
            continue;
        }

        let Some(t) = block_place(i.into()) else {
            continue;
        };
        *b |= t as u32;
        mark_dirty(&mut state.mesh, state.chunks_size, c);
        slot.count -= 1;
        if slot.count == 0 {
            slot.item_id = None;
        }
    }

    for d in &mut state.drones {
        let Command::Restack = d.command else {
            continue;
        };
        d.command = Command::Noop;

        fn f(a: &Inventory, b: &Inventory) -> Ordering {
            match (&a.item_id, &b.item_id) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            }
        }

        d.inventory.sort_unstable_by(f);
        for i in 0..d.inventory.len() {
            let mut dst = d.inventory[i];
            if dst.item_id.is_none() {
                break;
            }
            for src in &mut d.inventory[i..] {
                if src.item_id != dst.item_id {
                    break;
                }
                let n = src.count.min(Inventory::MAX_STACK - dst.count);
                dst.count += n;
                src.count -= n;
                if src.count == 0 {
                    src.item_id = None;
                }
                if dst.count == Inventory::MAX_STACK {
                    break;
                }
            }
        }
        d.inventory.sort_unstable_by(f);
    }

    for i in 0..state.drones.len() {
        let mut d = &mut state.drones[i];
        let Command::SendItem(dir, slot) = d.command else {
            continue;
        };
        d.command = Command::Noop;

        let Some(mut src) = d.inventory.get(slot as usize).copied() else {
            continue;
        };
        if src.item_id.is_none() {
            continue;
        }
        let Some(j) = dir
            .move_coord(&size, (d.x, d.y, d.z))
            .and_then(|c| state.rev_index.binary_search_by(|r| r.cmp_coord(&c)).ok())
            .map(|i| state.rev_index[i].i)
            .filter(|&j| i != j)
        else {
            continue;
        };
        d = &mut state.drones[j];
        Inventory::try_put_one(&mut d.inventory, &mut src);
        state.drones[i].inventory[slot as usize] = src;
    }

    for i in 0..state.drones.len() {
        let mut d = &mut state.drones[i];
        let Command::RecvItem(dir, slot) = d.command else {
            continue;
        };
        d.command = Command::Noop;

        let Some(mut dst) = d.inventory.get(slot as usize).copied() else {
            continue;
        };
        let Some(j) = dir
            .move_coord(&size, (d.x, d.y, d.z))
            .and_then(|c| state.rev_index.binary_search_by(|r| r.cmp_coord(&c)).ok())
            .map(|i| state.rev_index[i].i)
            .filter(|&j| i != j)
        else {
            continue;
        };
        d = &mut state.drones[j];

        for src in &mut d.inventory {
            match (src.item_id, dst.item_id) {
                (None, _) => (),
                (_, None) => {
                    (dst.item_id, dst.count) = (src.item_id, src.count);
                    (src.item_id, src.count) = (None, 0);
                    break;
                }
                (a, b) if a != b => (),
                _ => {
                    let n = src.count.min(Inventory::MAX_STACK - dst.count);
                    dst.count += n;
                    src.count -= n;
                    if src.count == 0 {
                        src.item_id = None;
                    }
                    if dst.count == Inventory::MAX_STACK {
                        break;
                    }
                }
            }
        }

        state.drones[i].inventory[slot as usize] = dst;
    }

    if has_move {
        move_drone(state);
    }
}

#[inline]
fn move_drone(state: &mut State) {
    state.move_index.sort_unstable_by(|a, b| {
        if let v @ (Ordering::Less | Ordering::Greater) = a.cmp(b) {
            return v;
        }
        let a = &state.drones[a.i];
        let b = &state.drones[b.i];
        match a.x.cmp(&b.x) {
            Ordering::Equal => match a.z.cmp(&b.z) {
                Ordering::Equal => a.y.cmp(&b.y),
                v => v,
            },
            v => v,
        }
    });

    for a in state.move_index.windows(2) {
        let [a, b] = <&[_; 2]>::try_from(a).unwrap();
        if a != b {
            continue;
        }
        let d = &mut state.drones[b.i];
        if matches!(d.command, Command::Move(_)) {
            d.command = Command::Noop;
        }
    }

    fn f(state: &mut State, i: usize) {
        let d = &state.drones[i];
        let r = MoveIndex {
            x: d.x,
            y: d.y,
            z: d.z,
            i,
        };
        let s = state.move_index.partition_point(|m| m < &r);
        if state.move_index.get(s) != Some(&r) {
            return;
        }
        let e = state.move_index[s + 1..].partition_point(|m| m <= &r);
        for mut j in s..s + 1 + e {
            j = state.move_index[j].i;
            let d = &mut state.drones[j];
            if (i == j) || !matches!(d.command, Command::Move(_)) {
                continue;
            }
            d.command = Command::Noop;
            f(state, j);
        }
    }
    for i in 0..state.rev_index.len() {
        if !matches!(state.drones[state.rev_index[i].i].command, Command::Move(_)) {
            f(state, i);
        }
    }

    for d in &state.drones {
        if matches!(d.command, Command::Move(_)) {
            state.data[(d.x, d.y, d.z)] &= !OCCUPIED_FLAG;
        }
    }
    for m in &state.move_index {
        let d = &mut state.drones[m.i];
        if matches!(d.command, Command::Move(_)) {
            state.data[(m.x, m.y, m.z)] |= OCCUPIED_FLAG;
            (d.x, d.y, d.z) = (m.x, m.y, m.z);
            d.command = Command::Noop;
        }
    }
}
