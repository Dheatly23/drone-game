use std::iter::repeat;
use std::ptr::dangling;

use glam::f32::{Vec2, Vec3, Vec4};

use level_state::{Block, LevelState, CHUNK_SIZE};
use util_wasm::log;

struct Render {
    vertex: Vec<Vec3>,
    normal: Vec<Vec3>,
    tangent: Vec<Vec4>,
    uv: Vec<Vec2>,
    index: Vec<u32>,
}

#[repr(C)]
pub struct ExportRender {
    dirty: u8,
    attr_len: usize,
    vertex_ptr: *const Vec3,
    normal_ptr: *const Vec3,
    tangent_ptr: *const Vec4,
    uv_ptr: *const Vec2,
    index_len: usize,
    index_ptr: *const u32,
}

static mut RENDER: Render = Render {
    vertex: Vec::new(),
    normal: Vec::new(),
    tangent: Vec::new(),
    uv: Vec::new(),
    index: Vec::new(),
};
static mut EXPORT_RENDER: ExportRender = ExportRender {
    dirty: 0,
    attr_len: 0,
    index_len: 0,
    vertex_ptr: dangling(),
    normal_ptr: dangling(),
    tangent_ptr: dangling(),
    uv_ptr: dangling(),
    index_ptr: dangling(),
};

pub fn render_chunk(level: &mut LevelState, x: usize, y: usize, z: usize) -> *const ExportRender {
    log(format_args!("coord: {x} {y} {z}"));
    let (render, export) = unsafe { (&mut *(&raw mut RENDER), &mut *(&raw mut EXPORT_RENDER)) };

    let (sx, sy, sz) = level.chunk_size();
    log(format_args!("size: {sx} {sy} {sz}"));
    if x >= sx || y >= sy || z >= sz {
        panic!("Index overflow");
    }
    let i = (y * sz + z) * sx + x;
    log(format_args!(
        "index: {i} chunks len: {}",
        level.chunks_mut().len()
    ));
    let c = &mut level.chunks_mut()[i];
    if !c.is_dirty() {
        export.dirty = 0;
        export.attr_len = 0;
        export.index_len = 0;
        return export;
    }
    c.mark_clean();
    let c = &level.chunks()[i];

    render.vertex.clear();
    render.normal.clear();
    render.tangent.clear();
    render.uv.clear();
    render.index.clear();

    let cl = if x < sx - 1 {
        Some(&level.chunks()[i + 1])
    } else {
        None
    };
    let cr = if x > 0 {
        Some(&level.chunks()[i - 1])
    } else {
        None
    };
    let cf = if z < sz - 1 {
        Some(&level.chunks()[i + sx])
    } else {
        None
    };
    let cb = if z > 0 {
        Some(&level.chunks()[i - sx])
    } else {
        None
    };
    let cu = if y < sy - 1 {
        Some(&level.chunks()[i + sx * sz])
    } else {
        None
    };
    let cd = if y > 0 {
        Some(&level.chunks()[i - sx * sz])
    } else {
        None
    };

    enum RenderType {
        Block { uv: Vec2, duv: Vec2 },
    }
    let mut f = |i: usize, x: usize, y: usize, z: usize, r: RenderType| {
        let coord = Vec3::new(x as _, y as _, z as _);
        match r {
            RenderType::Block { uv, duv } => draw_block(
                render,
                coord,
                uv,
                duv,
                [
                    match cl {
                        _ if x < CHUNK_SIZE - 1 => Some(&c.blocks()[i + 1]),
                        Some(c) => Some(c.get_block(0, y, z)),
                        None => None,
                    },
                    match cu {
                        _ if y < CHUNK_SIZE - 1 => Some(&c.blocks()[i + CHUNK_SIZE * CHUNK_SIZE]),
                        Some(c) => Some(c.get_block(x, 0, z)),
                        None => None,
                    },
                    match cf {
                        _ if z < CHUNK_SIZE - 1 => Some(&c.blocks()[i + CHUNK_SIZE]),
                        Some(c) => Some(c.get_block(x, y, 0)),
                        None => None,
                    },
                    match cr {
                        _ if x > 0 => Some(&c.blocks()[i - 1]),
                        Some(c) => Some(c.get_block(CHUNK_SIZE - 1, y, z)),
                        None => None,
                    },
                    match cd {
                        _ if y > 0 => Some(&c.blocks()[i - CHUNK_SIZE * CHUNK_SIZE]),
                        Some(c) => Some(c.get_block(x, CHUNK_SIZE - 1, z)),
                        None => None,
                    },
                    match cb {
                        _ if z > 0 => Some(&c.blocks()[i - CHUNK_SIZE]),
                        Some(c) => Some(c.get_block(x, y, CHUNK_SIZE - 1)),
                        None => None,
                    },
                ]
                .map(|v| !v.is_some_and(|b| b.get().is_solid())),
            ),
        }
    };

    let mut it = c.blocks().iter().enumerate();
    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let (i, v) = it.next().unwrap();
                let r = match v.get() {
                    Block::Dirt => RenderType::Block {
                        uv: Vec2::new(0., 0.) / 64.,
                        duv: Vec2::ONE / 64.,
                    },
                    Block::Grass => RenderType::Block {
                        uv: Vec2::new(1., 0.) / 64.,
                        duv: Vec2::ONE / 64.,
                    },
                    _ => continue,
                };
                f(i, x, y, z, r);
            }
        }
    }

    export.dirty = 1;
    export.attr_len = render.vertex.len();
    export.index_len = render.index.len();
    export.vertex_ptr = render.vertex.as_ptr();
    export.normal_ptr = render.normal.as_ptr();
    export.tangent_ptr = render.tangent.as_ptr();
    export.uv_ptr = render.uv.as_ptr();
    export.index_ptr = render.index.as_ptr();

    export
}

fn draw_block(this: &mut Render, c: Vec3, uv: Vec2, duv: Vec2, b: [bool; 6]) {
    let uv00 = uv;
    let uv10 = uv + Vec2::new(duv.x, 0.);
    let uv01 = uv + Vec2::new(0., duv.y);
    let uv11 = uv + duv;

    let c000 = c;
    let c100 = c000 + Vec3::X;
    let c010 = c000 + Vec3::Y;
    let c001 = c000 + Vec3::Z;
    let c110 = c100 + Vec3::Y;
    let c011 = c010 + Vec3::Z;
    let c101 = c001 + Vec3::X;
    let c111 = c110 + Vec3::Z;

    // Up
    if b[1] {
        let i = this.vertex.len() as u32;
        this.vertex.extend([c110, c010, c111, c011]);
        this.normal.extend(repeat(Vec3::Y).take(4));
        this.tangent.extend(repeat(Vec3::NEG_X.extend(1.)).take(4));
        this.uv.extend([uv00, uv10, uv01, uv11]);
        this.index
            .extend([0, 3, 1, 0, 2, 3].into_iter().map(|v| v + i));
    }

    // Down
    if b[4] {
        let i = this.vertex.len() as u32;
        this.vertex.extend([c101, c001, c100, c000]);
        this.normal.extend(repeat(Vec3::NEG_Y).take(4));
        this.tangent.extend(repeat(Vec3::NEG_X.extend(1.)).take(4));
        this.uv.extend([uv00, uv10, uv01, uv11]);
        this.index
            .extend([0, 3, 1, 0, 2, 3].into_iter().map(|v| v + i));
    }

    // Left
    if b[0] {
        let i = this.vertex.len() as u32;
        this.vertex.extend([c101, c100, c111, c110]);
        this.normal.extend(repeat(Vec3::X).take(4));
        this.tangent.extend(repeat(Vec3::NEG_Z.extend(1.)).take(4));
        this.uv.extend([uv00, uv10, uv01, uv11]);
        this.index
            .extend([0, 3, 1, 0, 2, 3].into_iter().map(|v| v + i));
    }

    // Right
    if b[3] {
        let i = this.vertex.len() as u32;
        this.vertex.extend([c000, c001, c010, c011]);
        this.normal.extend(repeat(Vec3::NEG_X).take(4));
        this.tangent.extend(repeat(Vec3::Z.extend(1.)).take(4));
        this.uv.extend([uv00, uv10, uv01, uv11]);
        this.index
            .extend([0, 3, 1, 0, 2, 3].into_iter().map(|v| v + i));
    }

    // Front
    if b[2] {
        let i = this.vertex.len() as u32;
        this.vertex.extend([c001, c101, c011, c111]);
        this.normal.extend(repeat(Vec3::Z).take(4));
        this.tangent.extend(repeat(Vec3::X.extend(1.)).take(4));
        this.uv.extend([uv00, uv10, uv01, uv11]);
        this.index
            .extend([0, 3, 1, 0, 2, 3].into_iter().map(|v| v + i));
    }

    // Back
    if b[5] {
        let i = this.vertex.len() as u32;
        this.vertex.extend([c100, c000, c110, c010]);
        this.normal.extend(repeat(Vec3::NEG_Z).take(4));
        this.tangent.extend(repeat(Vec3::NEG_X.extend(1.)).take(4));
        this.uv.extend([uv00, uv10, uv01, uv11]);
        this.index
            .extend([0, 3, 1, 0, 2, 3].into_iter().map(|v| v + i));
    }
}
