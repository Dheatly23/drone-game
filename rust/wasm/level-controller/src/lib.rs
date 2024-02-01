// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
#![allow(dead_code)]

mod blocks;
mod drone;
mod meshgen;
mod pubsub;

use std::ptr;
use std::rc::Rc;

use glam::f32::*;
use ndarray::{s, Array, Array3, Dimension};
use rand::{Rng, SeedableRng};
use rand_xoshiro::Xoshiro512StarStar;

#[derive(Debug, Default)]
struct Mesh {
    dirty: bool,
    vertex: Vec<Vec3>,
    normal: Vec<Vec3>,
    tangent: Vec<Vec4>,
    uv: Vec<Vec2>,
    index: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ExportMesh {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub dirty: bool,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex: *const Vec3,
    pub normal: *const Vec3,
    pub tangent: *const Vec4,
    pub uv: *const Vec2,
    pub index: *const u32,
}

impl ExportMesh {
    const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            z: 0,
            dirty: false,
            vertex_count: 0,
            index_count: 0,
            vertex: ptr::null(),
            normal: ptr::null(),
            tangent: ptr::null(),
            uv: ptr::null(),
            index: ptr::null(),
        }
    }
}

const OCCUPIED_FLAG: u32 = 0x8000_0000;

struct State {
    rng: Xoshiro512StarStar,
    tick_count: usize,
    data: Array3<u32>,

    chunks_size: usize,
    mesh: Array3<Mesh>,
    export_mesh: Array3<ExportMesh>,

    drones: Vec<drone::Drone>,
    pubsub: pubsub::PubSub,

    move_index: Vec<drone::MoveIndex>,
    rev_index: Vec<drone::MoveIndex>,
    key_cache: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ExportState {
    pub size_x: usize,
    pub size_y: usize,
    pub size_z: usize,
    pub data: *mut u32,

    pub mesh_count: usize,
    pub mesh: *const ExportMesh,

    pub drone_count: usize,
    pub drone: *mut drone::Drone,
}

impl State {
    fn new(
        seed: u64,
        size: [usize; 3],
        chunks_size: usize,
        drone_count: usize,
        tick_count: usize,
    ) -> Self {
        let mut data = Array::zeros(size);
        let shape = [
            (size[0] + (chunks_size - 1)) / chunks_size,
            (size[1] + (chunks_size - 1)) / chunks_size,
            (size[2] + (chunks_size - 1)) / chunks_size,
        ];
        let mesh = Array::from_shape_simple_fn(shape, || Mesh {
            dirty: true,
            ..Mesh::default()
        });
        let export_mesh = Array::from_shape_fn(shape, |(x, y, z)| ExportMesh {
            x: x * chunks_size,
            y: y * chunks_size,
            z: z * chunks_size,

            ..ExportMesh::new()
        });

        let mut drones = vec![drone::Drone::default(); drone_count];
        let mut pubsub = pubsub::PubSub::new();
        pubsub.add_subscribers(drone_count);

        for ((i, v), d) in data.indexed_iter_mut().zip(&mut drones) {
            (d.x, d.y, d.z) = i;
            *v |= OCCUPIED_FLAG;
        }

        Self {
            rng: Xoshiro512StarStar::seed_from_u64(seed),
            tick_count,
            data,
            chunks_size,
            mesh,
            export_mesh,
            drones,
            pubsub,
            move_index: vec![drone::MoveIndex::default(); drone_count],
            rev_index: vec![drone::MoveIndex::default(); drone_count],
            key_cache: Vec::new(),
        }
    }

    fn write_export(&mut self, export: &mut ExportState) {
        self.export_mesh.zip_mut_with(&self.mesh, |o, i| {
            *o = ExportMesh {
                dirty: i.dirty,
                vertex_count: i.vertex.len(),
                index_count: i.index.len(),

                vertex: i.vertex.as_ptr(),
                normal: i.normal.as_ptr(),
                tangent: i.tangent.as_ptr(),
                uv: i.uv.as_ptr(),
                index: i.index.as_ptr(),

                ..*o
            }
        });
        for m in &mut self.mesh {
            m.dirty = false;
        }

        (export.size_x, export.size_y, export.size_z) = self.data.raw_dim().into_pattern();
        export.data = self
            .data
            .as_slice_mut()
            .expect("Data is not C-contiguous")
            .as_mut_ptr();
        export.mesh_count = self.export_mesh.len();
        export.mesh = self
            .export_mesh
            .as_slice()
            .expect("Data is not C-contiguous")
            .as_ptr();
        export.drone_count = self.drones.len();
        export.drone = self.drones.as_mut_ptr();
    }
}

impl ExportState {
    const fn new() -> Self {
        Self {
            size_x: 0,
            size_y: 0,
            size_z: 0,
            data: ptr::null_mut(),
            mesh_count: 0,
            mesh: ptr::null(),
            drone_count: 0,
            drone: ptr::null_mut(),
        }
    }
}

static mut STATE: Option<State> = None;
static mut EXPORT: ExportState = ExportState::new();

fn write_export(state: &mut State) {
    unsafe { state.write_export(&mut EXPORT) }
}

#[no_mangle]
pub extern "C" fn init(
    seed: u64,
    size_x: usize,
    size_y: usize,
    size_z: usize,
    drone_count: usize,
    tick_count: usize,
) -> *mut ExportState {
    const CHUNKS_SIZE: usize = 16;

    unsafe {
        let mut state = State::new(
            seed,
            [size_x, size_y, size_z],
            CHUNKS_SIZE,
            drone_count,
            tick_count,
        );
        write_export(&mut state);
        STATE = Some(state);
        &mut EXPORT
    }
}

#[no_mangle]
pub extern "C" fn generate_mesh() {
    let state = unsafe { STATE.as_mut().unwrap() };

    let data = state.data.view();
    for ((x, y, z), mesh) in state.mesh.indexed_iter_mut() {
        //if !mesh.dirty {
        //    continue;
        //}
        meshgen::gen_mesh(
            data,
            state.chunks_size,
            [
                x * state.chunks_size,
                y * state.chunks_size,
                z * state.chunks_size,
            ],
            mesh,
        );
    }

    write_export(state);
}

#[no_mangle]
pub extern "C" fn step() {
    let state = unsafe { STATE.as_mut().unwrap() };

    drone::execute_commands(state);

    let (sx, sy, sz) = state.data.raw_dim().into_pattern();
    let mut n = 0;
    blocks::random_tick(
        &mut state.rng,
        |r| {
            if n >= state.tick_count {
                return None;
            }
            n += 1;
            Some((r.gen_range(0..sx), r.gen_range(0..sy), r.gen_range(0..sz)))
        },
        &mut state.data,
    );

    write_export(state);
}

#[no_mangle]
pub extern "C" fn mark_all_dirty() {
    let state = unsafe { STATE.as_mut().unwrap() };
    for m in &mut state.mesh {
        m.dirty = true;
    }
}

#[no_mangle]
pub extern "C" fn mark_dirty(
    mut sx: usize,
    mut sy: usize,
    mut sz: usize,
    mut ex: usize,
    mut ey: usize,
    mut ez: usize,
) {
    let state = unsafe { STATE.as_mut().unwrap() };

    let (x_, y_, z_) = state.data.raw_dim().into_pattern();
    if (ex == 0) || (ey == 0) || (ez == 0) || (sx >= x_) || (sy >= y_) || (sz >= z_) {
        return;
    }

    ex = (sx + (ex - 1)) / state.chunks_size + 1;
    ey = (sy + (ey - 1)) / state.chunks_size + 1;
    ez = (sz + (ez - 1)) / state.chunks_size + 1;
    sx /= state.chunks_size;
    sy /= state.chunks_size;
    sz /= state.chunks_size;

    for m in state
        .mesh
        .slice_mut(s![sx..ex.min(x_), sy..ey.min(y_), sz..ez.min(z_)])
    {
        m.dirty = true;
    }
}

#[no_mangle]
pub extern "C" fn update_all_drones() {
    let state = unsafe { STATE.as_mut().unwrap() };

    state.data &= !OCCUPIED_FLAG;
    for d in &state.drones {
        state.data[(d.x, d.y, d.z)] |= OCCUPIED_FLAG;
    }
}

#[link(wasm_import_module = "host")]
extern "C" {
    fn read_key(ptr: *mut u8);
    fn read_key_msg(key_ptr: *mut u8, msg_ptr: *mut u8);
    fn write_key_msg(key_len: usize, key_ptr: *const u8, msg_len: usize, msg_ptr: *const u8);
}

#[no_mangle]
pub extern "C" fn pubsub_pop(i: usize) {
    let state = unsafe { STATE.as_mut().unwrap() };

    if let Some((key, msg)) = state.pubsub[i].pop() {
        unsafe { write_key_msg(key.len(), key.as_ptr(), msg.len(), msg.as_ptr()) };
    }
}

#[no_mangle]
pub extern "C" fn pubsub_listen(i: usize, key_len: usize) {
    let state = unsafe { STATE.as_mut().unwrap() };

    state.key_cache.resize(key_len, 0);
    unsafe { read_key(state.key_cache.as_mut_ptr()) };

    state.pubsub.subscriber_listen(i, &*state.key_cache);
}

#[no_mangle]
pub extern "C" fn pubsub_publish(key_len: usize, msg_len: usize) {
    let state = unsafe { STATE.as_mut().unwrap() };

    state.key_cache.resize(key_len, 0);
    let msg = <Rc<[u8]>>::from(vec![0; msg_len]);
    unsafe { read_key_msg(state.key_cache.as_mut_ptr(), msg.as_ptr() as *mut _) };

    state.pubsub.publish(&*state.key_cache, msg);
}

#[no_mangle]
pub extern "C" fn pubsub_transfer() {
    let state = unsafe { STATE.as_mut().unwrap() };

    state.pubsub.transfer();
}
