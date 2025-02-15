use std::cmp::Ordering;
use std::mem::replace;

use rand::prelude::*;
use rand_xoshiro::Xoshiro256PlusPlus;

use level_state::{
    Block, BlockEntity, BlockEntityData, Command, Direction, LevelState, CHUNK_SIZE,
};

const UPDATE_RATE: usize = 32;

pub fn update(level: &mut LevelState) {
    drone_command(level);
    random_tick(level);
}

fn drone_command(level: &mut LevelState) {
    let (sx, sy, sz) = level.chunk_size();
    let (ex, ey, ez) = (sx * CHUNK_SIZE, sy * CHUNK_SIZE, sz * CHUNK_SIZE);

    // Move command
    let mut move_data = Vec::new();
    let mut end_map = Vec::new();
    for (id, v) in level.block_entities_mut().entries_mut() {
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
        end_map.push(([x, y, z], None));

        let Command::Move(dir) = d.command else {
            d.is_command_valid = matches!(d.command, Command::Noop);
            continue;
        };
        let dest = match dir {
            Direction::Left if x + 1 < ex => [x + 1, z, y],
            Direction::Up if y + 1 < ey => [x, z, y + 1],
            Direction::Forward if z + 1 < ez => [x, z + 1, y],
            Direction::Right if x > 0 => [x - 1, z, y],
            Direction::Down if y > 0 => [x, z, y - 1],
            Direction::Back if z > 0 => [x, z - 1, y],
            _ => {
                d.is_command_valid = false;
                continue;
            }
        };
        end_map.last_mut().unwrap().1 = Some(move_data.len());
        move_data.push((*id, [x, z, y], dest, true));
    }

    // Sort end mapping
    // Make sure None is put before Some(index)
    end_map.sort_unstable_by(|(ca, ia), (cb, ib)| match ca.cmp(cb) {
        Ordering::Equal => match (*ia, *ib) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(a), Some(b)) => move_data[a].1.cmp(&move_data[b].1),
        },
        v => v,
    });

    // Try to move
    let mut prev = None;
    for &(d, i) in &end_map {
        let prev = replace(&mut prev, Some(d));
        let Some(i) = i else {
            continue;
        };

        move_data[i].3 = prev.is_none_or(|v| v == d)
            && !level
                .get_chunk_mut(d[0] / CHUNK_SIZE, d[1] / CHUNK_SIZE, d[2] / CHUNK_SIZE)
                .get_block_mut(d[0] % CHUNK_SIZE, d[1] % CHUNK_SIZE, d[2] % CHUNK_SIZE)
                .get()
                .is_solid();
    }

    // Recursively un-move drones
    let mut any = true;
    while any {
        any = false;
        for i in 0..move_data.len() {
            let (_, _, d, true) = move_data[i] else {
                // Skip already failed drone
                continue;
            };

            let mut j = end_map.partition_point(|(c, _)| *c < d);
            while let Some((c, k)) = end_map.get(j) {
                if *c != d {
                    break;
                } else if !k.is_some_and(|i| move_data[i].3) {
                    move_data[i].3 = false;
                    any = true;
                    break;
                }
                j += 1;
            }
        }
    }

    // Move successful drones
    for (id, _, dest, v) in move_data {
        let Some(BlockEntity {
            data: BlockEntityData::Drone(d),
            x,
            y,
            z,
            ..
        }) = level.block_entities_mut().get_mut(&id)
        else {
            unreachable!("Block entity should be drone")
        };

        if !v {
            d.is_command_valid = false;
            continue;
        }

        d.is_command_valid = true;
        [*x, *y, *z] = dest;
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

fn random_tick(level: &mut LevelState) {
    let mut rng = Xoshiro256PlusPlus::from_os_rng();

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
