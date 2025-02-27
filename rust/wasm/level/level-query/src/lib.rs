#![allow(clippy::deref_addrof)]

use std::ops::ControlFlow;
use std::ptr::null;

use glam::f32::Vec3;
use hashbrown::hash_map::HashMap;
use rkyv::api::high::access;
use rkyv::rancor::Panic;

use level_state::{ArchivedLevelState, BlockEntityHasher, CHUNK_SIZE};
use util_wasm::read;

static mut LEVEL: Option<&'static ArchivedLevelState> = None;

#[unsafe(no_mangle)]
pub extern "C" fn update() {
    unsafe {
        *(&raw mut LEVEL) = None;
        *(&raw mut LEVEL) = Some(access::<ArchivedLevelState, Panic>(read()).unwrap());
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
struct RayPoint {
    x: usize,
    y: usize,
    z: usize,
}

#[repr(C)]
pub struct RayData {
    point: RayPoint,
    uuid: [u32; 4],
}

static mut RAY_RES: RayData = RayData {
    point: RayPoint { x: 0, y: 0, z: 0 },
    uuid: [0; 4],
};

#[unsafe(no_mangle)]
pub extern "C" fn query_ray(x: f32, y: f32, z: f32, dx: f32, dy: f32, dz: f32) -> *const RayData {
    let level = unsafe { (*(&raw const LEVEL)).unwrap() };

    let ControlFlow::Break(v) = get_ray_helper(level, Vec3::new(x, y, z), Vec3::new(dx, dy, dz))
    else {
        return null();
    };

    unsafe {
        *(&raw mut RAY_RES) = v;
    }
    &raw const RAY_RES
}

fn get_ray_helper(level: &ArchivedLevelState, d: Vec3, n: Vec3) -> ControlFlow<RayData> {
    let (sx, sy, sz) = level.chunk_size();

    if d.x <= i32::MIN as f32
        || d.x >= i32::MAX as f32
        || d.y <= i32::MIN as f32
        || d.y >= i32::MAX as f32
        || d.z <= i32::MIN as f32
        || d.z >= i32::MAX as f32
    {
        return ControlFlow::Continue(());
    }

    // Get block entity positions
    let be_pos = {
        let mut m = HashMap::with_hasher(BlockEntityHasher);
        m.extend(level.block_entities().entries().map(|(&k, v)| {
            (
                RayPoint {
                    x: v.x.to_native() as _,
                    y: v.y.to_native() as _,
                    z: v.z.to_native() as _,
                },
                k,
            )
        }));
        m
    };

    let check_fn = |x: isize, y: isize, z: isize| {
        if x < 0
            || y < 0
            || z < 0
            || (x as usize) / CHUNK_SIZE >= sx
            || (y as usize) / CHUNK_SIZE >= sy
            || (z as usize) / CHUNK_SIZE >= sz
        {
            return ControlFlow::Continue(());
        }

        let x = x as usize;
        let y = y as usize;
        let z = z as usize;
        let p = RayPoint { x, y, z };
        if let Some(v) = be_pos.get(&p) {
            let v = v.as_u128();
            ControlFlow::Break(RayData {
                point: p,
                uuid: [
                    (v & 0xffff_ffff) as u32,
                    ((v >> 32) & 0xffff_ffff) as u32,
                    ((v >> 64) & 0xffff_ffff) as u32,
                    ((v >> 96) & 0xffff_ffff) as u32,
                ],
            })
        } else if level
            .get_chunk(x / CHUNK_SIZE, y / CHUNK_SIZE, z / CHUNK_SIZE)
            .get_block(x % CHUNK_SIZE, y % CHUNK_SIZE, z % CHUNK_SIZE)
            .get()
            .is_solid()
        {
            ControlFlow::Break(RayData {
                point: p,
                uuid: [0; 4],
            })
        } else {
            ControlFlow::Continue(())
        }
    };

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Axis {
        X,
        Y,
        Z,
    }

    struct It {
        axis: Axis,
        cur: isize,
        pos: bool,
        t: f32,
    }

    impl It {
        fn calc_t(&mut self, d: &Vec3, n: &Vec3) {
            let v = (self.cur + if self.pos { 1 } else { 0 }) as f32;
            let (d, n) = match self.axis {
                Axis::X => (d.x, n.x),
                Axis::Y => (d.y, n.y),
                Axis::Z => (d.z, n.z),
            };
            self.t = if n == 0.0 { f32::INFINITY } else { (v - d) / n };
            if self.t.is_nan() || self.t == f32::NEG_INFINITY {
                self.t = f32::INFINITY;
            }
        }

        fn step(&mut self) {
            self.cur += if self.pos { 1 } else { -1 };
        }

        fn init(mut self, d: &Vec3, n: &Vec3) -> Self {
            loop {
                self.calc_t(d, n);
                if self.t >= 0.0 {
                    return self;
                }
                self.step();
            }
        }
    }

    let mut xi = It {
        axis: Axis::X,
        cur: d.x as isize,
        pos: n.x.is_sign_positive(),
        t: 0.0,
    }
    .init(&d, &n);
    let mut yi = It {
        axis: Axis::Y,
        cur: d.y as isize,
        pos: n.y.is_sign_positive(),
        t: 0.0,
    }
    .init(&d, &n);
    let mut zi = It {
        axis: Axis::Z,
        cur: d.z as isize,
        pos: n.z.is_sign_positive(),
        t: 0.0,
    }
    .init(&d, &n);

    check_fn(xi.cur, yi.cur, zi.cur)?;

    const MAX_RADIUS: f32 = 64.0;
    const fn filter_max_radius(v: &It) -> Option<(f32, Axis)> {
        if v.t <= MAX_RADIUS {
            return Some((v.t, v.axis));
        }
        None
    }

    while let Some((_, a)) =
        [&xi, &yi, &zi]
            .into_iter()
            .fold(None, |a, b| match (a, filter_max_radius(b)) {
                (None, None) => None,
                (Some(v), None) | (None, Some(v)) => Some(v),
                (Some(a), Some(b)) => Some(if a.0 > b.0 { b } else { a }),
            })
    {
        let i = match a {
            Axis::X => &mut xi,
            Axis::Y => &mut yi,
            Axis::Z => &mut zi,
        };
        i.step();
        i.calc_t(&d, &n);

        check_fn(xi.cur, yi.cur, zi.cur)?;
    }

    ControlFlow::Continue(())
}
