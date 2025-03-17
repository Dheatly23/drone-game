use std::cell::Cell;
use std::cmp::Ordering;
use std::mem::{replace, take};

use rand::distr::Bernoulli;
use rand::prelude::*;
use uuid::Uuid;

use level_state::{
    Block, BlockEntity, BlockEntityData, BreakCapability, CHUNK_SIZE, CentralTower, Command,
    Direction, Drone, DroneCapabilityFlags, InventoryOp, InventoryType, Item, ItemSlot, ItemStack,
    LevelState,
};
use util_wasm::log;

const UPDATE_RATE: usize = 32;

pub fn update<R: RngCore>(level: &mut LevelState, rng: &mut R) {
    drone_command(level, rng);
    random_tick(level, rng);
}

fn drone_command<R: RngCore>(level: &mut LevelState, rng: &mut R) {
    let (sx, sy, sz) = level.chunk_size();
    let (ex, ey, ez) = (sx * CHUNK_SIZE, sy * CHUNK_SIZE, sz * CHUNK_SIZE);

    let move_coord =
        |x: usize, y: usize, z: usize, be: Option<&BlockEntityData>, dir: Direction| {
            let (dx, dy, dz) = match be {
                Some(BlockEntityData::CentralTower(_)) => match dir {
                    Direction::Left => (2, 0, 0),
                    Direction::Up => (0, 3, 0),
                    Direction::Forward => (0, 0, 2),
                    Direction::Right => (-2, 0, 0),
                    Direction::Down => (0, -1, 0),
                    Direction::Back => (0, 0, -2),

                    Direction::ForwardLeft => (2, 0, 2),
                    Direction::ForwardRight => (-2, 0, 2),
                    Direction::BackLeft => (2, 0, -2),
                    Direction::BackRight => (-2, 0, -2),
                    Direction::UpLeft => (2, 3, 0),
                    Direction::UpRight => (-2, 3, 0),
                    Direction::UpForward => (0, 3, 2),
                    Direction::UpBack => (0, 3, -2),
                    Direction::DownLeft => (2, -1, 0),
                    Direction::DownRight => (-2, -1, 0),
                    Direction::DownForward => (0, -1, 2),
                    Direction::DownBack => (0, -1, -2),

                    _ => return None,
                },
                _ => match dir {
                    Direction::Left => (1, 0, 0),
                    Direction::Up => (0, 1, 0),
                    Direction::Forward => (0, 0, 1),
                    Direction::Right => (-1, 0, 0),
                    Direction::Down => (0, -1, 0),
                    Direction::Back => (0, 0, -1),

                    Direction::ForwardLeft => (1, 0, 1),
                    Direction::ForwardRight => (-1, 0, 1),
                    Direction::BackLeft => (1, 0, -1),
                    Direction::BackRight => (-1, 0, -1),
                    Direction::UpLeft => (1, 1, 0),
                    Direction::UpRight => (-1, 1, 0),
                    Direction::UpForward => (0, 1, 1),
                    Direction::UpBack => (0, 1, -1),
                    Direction::DownLeft => (1, -1, 0),
                    Direction::DownRight => (-1, -1, 0),
                    Direction::DownForward => (0, -1, 1),
                    Direction::DownBack => (0, -1, -1),

                    _ => return None,
                },
            };

            let x = x.checked_add_signed(dx)?;
            let y = y.checked_add_signed(dy)?;
            let z = z.checked_add_signed(dz)?;
            if x >= ex || y >= ey || z >= ez {
                return None;
            }
            Some((x, y, z))
        };

    let mut coord_map = level
        .block_entities()
        .entries()
        .map(|(&id, v)| (v.x, v.y, v.z, id))
        .collect::<Vec<_>>();
    coord_map.sort_unstable();
    let get_coord = move |x: usize, y: usize, z: usize| {
        coord_map
            .binary_search_by(|(xb, yb, zb, _)| {
                x.cmp(xb).then_with(|| y.cmp(yb)).then_with(|| z.cmp(zb))
            })
            .ok()
            .map(|i| coord_map[i].3)
    };

    fn get_inventory(target: &mut BlockEntityData, ty: InventoryType) -> Option<&mut [ItemSlot]> {
        match ty {
            InventoryType::Inventory => match target {
                BlockEntityData::Drone(v) => Some(&mut v.inventory),
                BlockEntityData::CentralTower(v) => Some(&mut v.inventory),
                _ => None,
            }
            .map(|v| &mut v[..]),
            InventoryType::ExtInventory => match target {
                BlockEntityData::Drone(v) => v.capabilities.ext_inventory.as_mut(),
                BlockEntityData::CentralTower(v) => v.capabilities.ext_inventory.as_mut(),
                _ => None,
            }
            .map(|v| &mut v[..]),
            _ => None,
        }
    }

    fn get_slot(
        target: &mut BlockEntityData,
        ty: InventoryType,
        slot: usize,
    ) -> Option<&mut ItemSlot> {
        get_inventory(target, ty)?.get_mut(slot)
    }

    let get_inventory_at =
        |level: &mut LevelState, x: usize, y: usize, z: usize, ty: InventoryType, slot: usize| {
            if let Some(r) = get_coord(x, y, z).and_then(|id| {
                get_slot(&mut level.block_entities_mut().get_mut(&id)?.data, ty, slot)
            }) {
                return Some(r as *mut ItemSlot);
            }

            let b = level.get_block(x, y, z).get();
            if let Some(r) = CentralTower::get_central_block_offset(b).and_then(|(dx, dy, dz)| {
                let x = x.checked_add_signed(-dx)?;
                let y = y.checked_add_signed(-dy)?;
                let z = z.checked_add_signed(-dz)?;

                if x >= ex || y >= ey || z >= ez {
                    return None;
                }

                get_slot(
                    &mut level
                        .block_entities_mut()
                        .get_mut(&get_coord(x, y, z)?)?
                        .data,
                    ty,
                    slot,
                )
            }) {
                return Some(r as *mut ItemSlot);
            }

            None
        };

    // Inventory ops command
    {
        enum InventoryOpEnum {
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
        }

        struct InventoryOpData {
            id: Uuid,
            x: usize,
            y: usize,
            z: usize,

            op: InventoryOpEnum,
        }

        let mut cmd_data = Vec::new();
        for (&id, v) in level.block_entities_mut().entries_mut() {
            let BlockEntity {
                data:
                    BlockEntityData::Drone(Drone {
                        command: ref mut cmd,
                        is_command_valid: ref mut valid,
                        ..
                    })
                    | BlockEntityData::CentralTower(CentralTower {
                        command: ref mut cmd,
                        is_command_valid: ref mut valid,
                        ..
                    }),
                x,
                y,
                z,
                ..
            } = *v
            else {
                continue;
            };

            let op = match *cmd {
                Command::PullInventory {
                    direction,
                    src_inv,
                    src_slot,
                    dst_inv,
                    dst_slot,
                    count,
                } => InventoryOpEnum::PullInventory {
                    direction,
                    src_inv,
                    src_slot,
                    dst_inv,
                    dst_slot,
                    count,
                },
                Command::PushInventory {
                    direction,
                    src_inv,
                    src_slot,
                    dst_inv,
                    dst_slot,
                    count,
                } => InventoryOpEnum::PushInventory {
                    direction,
                    src_inv,
                    src_slot,
                    dst_inv,
                    dst_slot,
                    count,
                },
                Command::InventoryOps(ref mut v) => InventoryOpEnum::InventoryOps(take(v)),
                _ => continue,
            };
            *valid = true;
            cmd_data.push(InventoryOpData { id, x, y, z, op });
        }

        cmd_data.sort_unstable_by(|a, b| {
            match (&a.op, &b.op) {
                (InventoryOpEnum::PullInventory { .. }, InventoryOpEnum::PullInventory { .. })
                | (InventoryOpEnum::PushInventory { .. }, InventoryOpEnum::PushInventory { .. })
                | (InventoryOpEnum::InventoryOps(_), InventoryOpEnum::InventoryOps(_)) => (),
                (InventoryOpEnum::PullInventory { .. }, _) => return Ordering::Less,
                (_, InventoryOpEnum::PullInventory { .. }) => return Ordering::Greater,
                (InventoryOpEnum::PushInventory { .. }, _) => return Ordering::Less,
                (_, InventoryOpEnum::PushInventory { .. }) => return Ordering::Greater,
            }

            a.x.cmp(&b.x)
                .then_with(|| a.z.cmp(&b.z))
                .then_with(|| a.y.cmp(&b.y))
                .then_with(|| a.id.cmp(&b.id))
        });

        for InventoryOpData { id, x, y, z, op } in cmd_data {
            let this = level.block_entities_mut().get_mut(&id);

            match op {
                InventoryOpEnum::PullInventory {
                    direction,
                    src_inv,
                    src_slot,
                    dst_inv,
                    dst_slot,
                    count,
                } => {
                    let Some((dx, dy, dz)) =
                        move_coord(x, y, z, this.as_deref().map(|v| &v.data), direction)
                    else {
                        continue;
                    };

                    let Some(dst) = this
                        .and_then(|v| get_inventory(&mut v.data, dst_inv)?.get_mut(dst_slot))
                        .map(|v| v as *mut ItemSlot)
                    else {
                        continue;
                    };
                    let Some(src) = get_inventory_at(level, dx, dy, dz, src_inv, src_slot) else {
                        continue;
                    };

                    assert_ne!(src, dst);
                    // SAFETY: Source and destination slot is not moved.
                    unsafe {
                        (*dst).transfer_slot(
                            &mut *src,
                            (if count == 0 { None } else { Some(count as _) }).as_mut(),
                        );
                    }
                }
                InventoryOpEnum::PushInventory {
                    direction,
                    src_inv,
                    src_slot,
                    dst_inv,
                    dst_slot,
                    count,
                } => {
                    let Some((dx, dy, dz)) =
                        move_coord(x, y, z, this.as_deref().map(|v| &v.data), direction)
                    else {
                        continue;
                    };

                    let Some(src) = this
                        .and_then(|v| get_inventory(&mut v.data, src_inv)?.get_mut(src_slot))
                        .map(|v| v as *mut ItemSlot)
                    else {
                        continue;
                    };
                    let Some(dst) = get_inventory_at(level, dx, dy, dz, dst_inv, dst_slot) else {
                        continue;
                    };

                    assert_ne!(src, dst);
                    // SAFETY: Source and destination slot is not moved and distinct.
                    unsafe {
                        (*dst).transfer_slot(
                            &mut *src,
                            (if count == 0 { None } else { Some(count as _) }).as_mut(),
                        );
                    }
                }
                InventoryOpEnum::InventoryOps(v) => {
                    let Some(BlockEntity { data: d, .. }) = this else {
                        unreachable!("block entity should exist")
                    };

                    for op in v {
                        match op {
                            InventoryOp::Swap { src, dst } => {
                                let (Some(src), Some(dst)) = (
                                    get_inventory(d, src.inventory)
                                        .and_then(|v| v.get_mut(src.slot))
                                        .map(|v| v as *mut ItemSlot),
                                    get_inventory(d, dst.inventory)
                                        .and_then(|v| v.get_mut(dst.slot))
                                        .map(|v| v as *mut ItemSlot),
                                ) else {
                                    continue;
                                };

                                if src == dst {
                                    continue;
                                }
                                // SAFETY: Source and destination slot is not moved and distinct.
                                unsafe {
                                    (*dst).swap_slot(&mut *src);
                                }
                            }
                            InventoryOp::Transfer { src, dst, count } => {
                                let (Some(src), Some(dst)) = (
                                    get_inventory(d, src.inventory)
                                        .and_then(|v| v.get_mut(src.slot))
                                        .map(|v| v as *mut ItemSlot),
                                    get_inventory(d, dst.inventory)
                                        .and_then(|v| v.get_mut(dst.slot))
                                        .map(|v| v as *mut ItemSlot),
                                ) else {
                                    continue;
                                };

                                if src == dst {
                                    continue;
                                }
                                // SAFETY: Source and destination slot is not moved and distinct.
                                unsafe {
                                    (*dst).transfer_slot(
                                        &mut *src,
                                        (if count == 0 { None } else { Some(count as _) }).as_mut(),
                                    );
                                }
                            }
                            InventoryOp::Pull { src, dst, count } => {
                                if src == dst.inventory {
                                    continue;
                                }
                                let (Some(src), Some(dst)) = (
                                    get_inventory(d, src).map(|v| v as *mut [ItemSlot]),
                                    get_inventory(d, dst.inventory)
                                        .and_then(|v| v.get_mut(dst.slot))
                                        .map(|v| v as *mut ItemSlot),
                                ) else {
                                    continue;
                                };

                                // SAFETY: Source and destination slot is not moved and distinct.
                                unsafe {
                                    (*dst).pull_inventory(
                                        &mut *src,
                                        (if count == 0 { None } else { Some(count) }).as_mut(),
                                    );
                                }
                            }
                            InventoryOp::Push { src, dst, count } => {
                                if src.inventory == dst {
                                    continue;
                                }
                                let (Some(src), Some(dst)) = (
                                    get_inventory(d, src.inventory)
                                        .and_then(|v| v.get_mut(src.slot))
                                        .map(|v| v as *mut ItemSlot),
                                    get_inventory(d, dst).map(|v| v as *mut [ItemSlot]),
                                ) else {
                                    continue;
                                };

                                // SAFETY: Source and destination slot is not moved and distinct.
                                unsafe {
                                    (*src).push_inventory(
                                        &mut *dst,
                                        (if count == 0 { None } else { Some(count) }).as_mut(),
                                    );
                                }
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    }

    // Break & mine command
    {
        struct BreakMineData {
            id: Uuid,
            valid: bool,
            x: usize,
            y: usize,
            z: usize,

            is_mine: bool,
            is_silk: bool,
            dir: Direction,
        }

        let mut cmd_data = Vec::new();
        for (&id, v) in level.block_entities().entries() {
            let BlockEntity {
                data: BlockEntityData::Drone(ref d),
                x,
                y,
                z,
                ..
            } = *v
            else {
                continue;
            };

            let (is_mine, dir) = match d.command {
                Command::Break(dir) => (false, dir),
                Command::Mine(dir) => (true, dir),
                _ => continue,
            };
            cmd_data.push(BreakMineData {
                id,
                x,
                y,
                z,
                is_mine,
                is_silk: d
                    .capabilities
                    .flags
                    .contains(DroneCapabilityFlags::SilkTouch),
                dir,
                valid: true,
            });
        }

        cmd_data.sort_unstable_by(|a, b| {
            a.is_mine
                .cmp(&b.is_mine)
                .then_with(|| a.x.cmp(&b.x))
                .then_with(|| a.z.cmp(&b.z))
                .then_with(|| a.y.cmp(&b.y))
                .then_with(|| a.id.cmp(&b.id))
        });

        for &mut BreakMineData {
            ref id,
            ref mut valid,
            x,
            y,
            z,
            is_mine,
            is_silk,
            dir,
        } in &mut cmd_data
        {
            let Some((x, y, z)) = move_coord(x, y, z, None, dir) else {
                *valid = false;
                continue;
            };

            let mut v: Box<[_]> = if is_mine {
                match level.get_block(x, y, z).get() {
                    Block::IronOre => {
                        let Some(BlockEntity {
                            data: BlockEntityData::IronOre(v),
                            ..
                        }) = get_coord(x, y, z)
                            .and_then(|id| level.block_entities_mut().get_mut(&id))
                        else {
                            unreachable!("block entity should be iron ore")
                        };

                        if v.quantity > 0 && rng.sample(Bernoulli::new(0.8).unwrap()) {
                            v.quantity -= 1;
                            Box::new([ItemStack {
                                item: Item::IronOre,
                                count: 1,
                            }])
                        } else {
                            Box::new([])
                        }
                    }
                    _ => {
                        *valid = false;
                        continue;
                    }
                }
            } else {
                let Some(v) =
                    level.break_block(x, y, z, BreakCapability::new(&mut *rng).silk_touch(is_silk))
                else {
                    *valid = false;
                    continue;
                };

                v
            };

            let Some(BlockEntity {
                data: BlockEntityData::Drone(d),
                ..
            }) = level.block_entities_mut().get_mut(id)
            else {
                unreachable!("block entity should be drone")
            };

            if ItemStack::put_inventory(&mut v, &mut d.inventory[..]) {
                if let Some(d) = &mut d.capabilities.ext_inventory {
                    ItemStack::put_inventory(&mut v, &mut d[..]);
                }
            }
        }

        for BreakMineData { id, valid, .. } in cmd_data {
            let Some(BlockEntity {
                data: BlockEntityData::Drone(d),
                ..
            }) = level.block_entities_mut().get_mut(&id)
            else {
                unreachable!("block entity should be drone")
            };

            d.is_command_valid = valid;
        }
    }

    drop(get_coord);

    // Move command
    {
        #[derive(Debug)]
        struct MoveData {
            id: Uuid,
            moving: Cell<bool>,

            sx: usize,
            sy: usize,
            sz: usize,
            ex: usize,
            ey: usize,
            ez: usize,
            flying: bool,
        }

        #[derive(Debug)]
        struct EndMap {
            x: usize,
            y: usize,
            z: usize,
            i: Option<usize>,
        }

        let mut move_data = Vec::new();
        let mut end_map = Vec::new();
        for (&id, v) in level.block_entities_mut().entries_mut() {
            let BlockEntity {
                data: BlockEntityData::Drone(ref mut d),
                x,
                y,
                z,
                ..
            } = *v
            else {
                continue;
            };
            end_map.push(EndMap { x, y, z, i: None });
            log(format_args!("{x} {y} {z} {d:?}"));

            let Command::Move(dir) = d.command else {
                continue;
            };
            if !d.capabilities.flags.contains(DroneCapabilityFlags::Moving) {
                d.is_command_valid = false;
                continue;
            }
            let Some((ex, ey, ez)) = move_coord(x, y, z, None, dir) else {
                d.is_command_valid = false;
                continue;
            };
            *end_map.last_mut().unwrap() = EndMap {
                x: ex,
                y: ey,
                z: ez,
                i: Some(move_data.len()),
            };
            move_data.push(MoveData {
                id,
                moving: Cell::new(true),

                sx: x,
                sy: y,
                sz: z,
                ex,
                ey,
                ez,
                flying: d.capabilities.flags.contains(DroneCapabilityFlags::Flying),
            });
        }
        log(format_args!("start: {move_data:?}"));

        // Sort end mapping
        // Make sure None is put before Some(index)
        end_map.sort_unstable_by(|a, b| {
            a.x.cmp(&b.x)
                .then_with(|| a.z.cmp(&b.z))
                .then_with(|| a.y.cmp(&b.y))
                .then_with(|| match (a.i, b.i) {
                    (None, None) => Ordering::Equal,
                    (None, Some(_)) => Ordering::Less,
                    (Some(_), None) => Ordering::Greater,
                    (Some(a), Some(b)) => {
                        let a = &move_data[a];
                        let b = &move_data[b];
                        a.sx.cmp(&b.sx)
                            .then_with(|| a.sz.cmp(&b.sz))
                            .then_with(|| a.sy.cmp(&b.sy))
                            .then_with(|| a.id.cmp(&b.id))
                    }
                })
        });
        log(format_args!("{end_map:?}"));

        // Try to move
        let mut stack = Vec::with_capacity(end_map.len());
        let mut prev = None;
        for v in &end_map {
            let prev = replace(&mut prev, Some((v.x, v.y, v.z)));
            let Some(i) = v.i else {
                continue;
            };
            let m = &move_data[i];

            if prev.is_some_and(|(x, y, z)| x == v.x && y == v.y && z == v.z)
                || level.get_block(v.x, v.y, v.z).get().is_solid()
                || (!m.flying && v.y != 0 && !level.get_block(v.x, v.y - 1, v.z).get().is_solid())
            {
                log(format_args!("Failed: {m:?}"));
                m.moving.set(false);
                stack.push(m);
            }
        }

        // Recursively un-move drones
        while let Some(&MoveData {
            sx: x,
            sy: y,
            sz: z,
            ..
        }) = stack.pop()
        {
            let i = end_map.partition_point(|t| {
                t.x.cmp(&x)
                    .then_with(|| t.z.cmp(&z))
                    .then_with(|| t.y.cmp(&y))
                    == Ordering::Less
            });
            for t in &end_map[i..] {
                if t.x != x || t.y != y || t.z != z {
                    break;
                }
                let Some(i) = t.i else {
                    continue;
                };

                let v = &move_data[i];
                if v.moving.replace(false) {
                    log(format_args!("Failed: {v:?}"));
                    stack.push(v);
                }
            }
        }
        log(format_args!("end: {move_data:?}"));

        // Move successful drones
        for MoveData {
            id,
            moving,
            ex,
            ey,
            ez,
            ..
        } in move_data
        {
            let Some(BlockEntity {
                data: BlockEntityData::Drone(d),
                x,
                y,
                z,
                ..
            }) = level.block_entities_mut().get_mut(&id)
            else {
                unreachable!("block entity should be drone")
            };

            if !moving.into_inner() {
                d.is_command_valid = false;
                continue;
            }

            d.is_command_valid = true;
            *x = ex;
            *y = ey;
            *z = ez;
        }
    }

    // Clear all drone commands
    for (_, v) in level.block_entities_mut().entries_mut() {
        // For now mark all drones as dirty
        v.mark_dirty();

        let BlockEntityData::Drone(v) = &mut v.data else {
            continue;
        };
        v.command = Command::Noop;
    }
}

fn random_tick<R: RngCore>(level: &mut LevelState, rng: &mut R) {
    let (sx, sy, sz) = level.chunk_size();
    let mut c = 0;
    for cy in 0..sy {
        for cz in 0..sz {
            for cx in 0..sx {
                for _ in 0..UPDATE_RATE {
                    let x = rng.random_range(..CHUNK_SIZE);
                    let y = rng.random_range(..CHUNK_SIZE);
                    let z = rng.random_range(..CHUNK_SIZE);
                    tick_block(level, cx, cy, cz, c, x, y, z);
                }
                c += 1;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn tick_block(
    level: &mut LevelState,
    cx: usize,
    cy: usize,
    cz: usize,
    c: usize,
    x: usize,
    y: usize,
    z: usize,
) {
    let (_, sy, _) = level.chunk_size();
    let i = (y * CHUNK_SIZE + z) * CHUNK_SIZE + x;

    match level.chunks()[c].blocks()[i].get() {
        Block::Grass => {
            let tb = if y < CHUNK_SIZE - 1 {
                level.chunks()[c].blocks()[i + CHUNK_SIZE * CHUNK_SIZE].get()
            } else if cy < sy - 1 {
                level.chunks()[c + cx * cz].blocks()[z * CHUNK_SIZE + x].get()
            } else {
                Block::Air
            };

            if tb.is_full_block() {
                let c = &mut level.chunks_mut()[c];
                c.blocks_mut()[i].set(Block::Dirt);
                c.mark_dirty();
            }
        }
        Block::Dirt => {
            let tb = if y < CHUNK_SIZE - 1 {
                level.chunks()[c].blocks()[i + CHUNK_SIZE * CHUNK_SIZE].get()
            } else if cy < sy - 1 {
                level.chunks()[c + cx * cz].blocks()[z * CHUNK_SIZE + x].get()
            } else {
                Block::Air
            };

            if !tb.is_full_block()
                && scan_block(
                    level,
                    cx * CHUNK_SIZE + x,
                    cy * CHUNK_SIZE + y,
                    cz * CHUNK_SIZE + z,
                    3,
                    Block::Grass,
                )
            {
                let c = &mut level.chunks_mut()[c];
                c.blocks_mut()[i].set(Block::Grass);
                c.mark_dirty();
            }
        }
        _ => (),
    }
}

fn scan_block(level: &mut LevelState, x: usize, y: usize, z: usize, r: usize, b: Block) -> bool {
    let (sx, sy, sz) = level.chunk_size();
    let step_y = sx * sz;

    let xl = x.saturating_sub(r);
    let xu = (x + r).min(sx * CHUNK_SIZE);
    let yl = y.saturating_sub(r);
    let yu = (y + r).min(sy * CHUNK_SIZE);
    let zl = z.saturating_sub(r);
    let zu = (z + r).min(sz * CHUNK_SIZE);

    let cxr = xl / CHUNK_SIZE..xu.div_ceil(CHUNK_SIZE);
    let cyr = yl / CHUNK_SIZE..yu.div_ceil(CHUNK_SIZE);
    let czr = zl / CHUNK_SIZE..zu.div_ceil(CHUNK_SIZE);
    let ciz = czr.start * sx;

    let mut ciy = cyr.start * step_y;
    for cy in cyr.clone() {
        let yr = if cy == cyr.start { yl % CHUNK_SIZE } else { 0 }..if cy == cyr.end - 1 {
            ((yu as isize - 1) % CHUNK_SIZE as isize) as usize + 1
        } else {
            CHUNK_SIZE
        };
        let mut ciz = ciy + ciz;
        for cz in czr.clone() {
            let zr = if cz == czr.start { zl % CHUNK_SIZE } else { 0 }..if cz == czr.end - 1 {
                ((zu as isize - 1) % CHUNK_SIZE as isize) as usize + 1
            } else {
                CHUNK_SIZE
            };
            for cx in cxr.clone() {
                let xr = if cx == cxr.start { xl % CHUNK_SIZE } else { 0 }..if cx == cxr.end - 1 {
                    ((xu as isize - 1) % CHUNK_SIZE as isize) as usize + 1
                } else {
                    CHUNK_SIZE
                };

                let c = &level.chunks()[ciz + cx];
                for y in yr.clone() {
                    for z in zr.clone() {
                        for x in xr.clone() {
                            if c.get_block(x, y, z).get() == b {
                                return true;
                            }
                        }
                    }
                }
            }
            ciz += sx;
        }
        ciy += step_y;
    }

    false
}
