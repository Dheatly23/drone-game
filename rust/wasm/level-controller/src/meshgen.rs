use std::f32::consts;
use std::iter;

use glam::f32::*;
use ndarray::ArrayView3;

use super::blocks::{block_type, block_uv, BlockType};
use super::Mesh;

const DIV_U: f32 = 1. / 8.0;
const DIV_V: f32 = 1. / 8.0;

pub fn gen_mesh(data: ArrayView3<u32>, size: usize, [sx, sy, sz]: [usize; 3], mesh: &mut Mesh) {
    mesh.vertex.clear();
    mesh.normal.clear();
    mesh.tangent.clear();
    mesh.uv.clear();
    mesh.index.clear();

    let ex = (sx + size).min(data.raw_dim()[0]);
    let ey = (sy + size).min(data.raw_dim()[1]);
    let ez = (sz + size).min(data.raw_dim()[2]);

    let mut f = |x, y, z| {
        let b = (data[(x, y, z)] & 0xff) as u8;
        match block_type(b) {
            BlockType::Empty => return,
            BlockType::Blade => {
                let [u, v] = block_uv(b);
                let u = (u as f32) * DIV_U;
                let v = (v as f32) * DIV_V;
                let uv1 = Vec2::new(u, v);
                let uv2 = Vec2::new(u + DIV_U, v);
                let uv3 = Vec2::new(u, v + DIV_V);
                let uv4 = Vec2::new(u + DIV_U, v + DIV_V);

                let i = mesh.vertex.len() as u32;
                mesh.vertex.extend(
                    iter::repeat([
                        Vec3::new((x - sx) as _, (y - sy) as _, (z - sz) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx) as _, (y - sy + 1) as _, (z - sz) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy) as _, (z - sz) as _),
                        Vec3::new((x - sx) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy + 1) as _, (z - sz) as _),
                    ])
                    .take(2)
                    .flatten(),
                );
                mesh.normal.extend(
                    [
                        Vec3::new(consts::FRAC_1_SQRT_2, 0., -consts::FRAC_1_SQRT_2),
                        Vec3::new(-consts::FRAC_1_SQRT_2, 0., -consts::FRAC_1_SQRT_2),
                        Vec3::new(-consts::FRAC_1_SQRT_2, 0., consts::FRAC_1_SQRT_2),
                        Vec3::new(consts::FRAC_1_SQRT_2, 0., consts::FRAC_1_SQRT_2),
                    ]
                    .into_iter()
                    .flat_map(|v| iter::repeat(v).take(4)),
                );
                mesh.tangent.extend(
                    [
                        Vec4::new(-consts::FRAC_1_SQRT_2, 0., -consts::FRAC_1_SQRT_2, 1.),
                        Vec4::new(-consts::FRAC_1_SQRT_2, 0., consts::FRAC_1_SQRT_2, 1.),
                        Vec4::new(consts::FRAC_1_SQRT_2, 0., consts::FRAC_1_SQRT_2, 1.),
                        Vec4::new(consts::FRAC_1_SQRT_2, 0., -consts::FRAC_1_SQRT_2, 1.),
                    ]
                    .into_iter()
                    .flat_map(|v| iter::repeat(v).take(4)),
                );
                mesh.uv.extend([uv2, uv1, uv4, uv3]);
                mesh.uv.extend_from_within(mesh.uv.len() - 4..);
                mesh.uv.extend([uv1, uv2, uv3, uv4]);
                mesh.uv.extend_from_within(mesh.uv.len() - 4..);
                mesh.index.extend(
                    [i, i + 4]
                        .into_iter()
                        .flat_map(|i| [i, i + 1, i + 3, i, i + 3, i + 2]),
                );
                mesh.index.extend(
                    [i + 8, i + 12]
                        .into_iter()
                        .flat_map(|i| [i + 1, i, i + 2, i + 1, i + 2, i + 3]),
                );
            }
            BlockType::Full => {
                let [u, v] = block_uv(b);
                let u = (u as f32) * DIV_U;
                let v = (v as f32) * DIV_V;
                let uv1 = Vec2::new(u, v);
                let uv2 = Vec2::new(u + DIV_U, v);
                let uv3 = Vec2::new(u, v + DIV_V);
                let uv4 = Vec2::new(u + DIV_U, v + DIV_V);

                // Up
                if (y + 1 >= ey)
                    || (block_type((data[(x, y + 1, z)] & 0xff) as u8) != BlockType::Full)
                {
                    let i = mesh.vertex.len() as u32;
                    mesh.vertex.extend([
                        Vec3::new((x - sx) as _, (y - sy + 1) as _, (z - sz) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy + 1) as _, (z - sz) as _),
                        Vec3::new((x - sx) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                    ]);
                    mesh.normal.extend(iter::repeat(Vec3::Y).take(4));
                    mesh.tangent
                        .extend(iter::repeat(Vec3::NEG_X.extend(-1.)).take(4));
                    mesh.uv.extend([uv2, uv1, uv4, uv3]);
                    mesh.index.extend([i, i + 1, i + 3, i, i + 3, i + 2]);
                }

                // Down
                if (y == 0) || (block_type((data[(x, y - 1, z)] & 0xff) as u8) != BlockType::Full) {
                    let i = mesh.vertex.len() as u32;
                    mesh.vertex.extend([
                        Vec3::new((x - sx) as _, (y - sy) as _, (z - sz) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy) as _, (z - sz) as _),
                        Vec3::new((x - sx) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy) as _, (z - sz + 1) as _),
                    ]);
                    mesh.normal.extend(iter::repeat(Vec3::NEG_Y).take(4));
                    mesh.tangent
                        .extend(iter::repeat(Vec3::NEG_X.extend(1.)).take(4));
                    mesh.uv.extend([uv2, uv1, uv4, uv3]);
                    mesh.index.extend([i + 1, i, i + 2, i + 1, i + 2, i + 3]);
                }

                // Left
                if (x + 1 >= ex)
                    || (block_type((data[(x + 1, y, z)] & 0xff) as u8) != BlockType::Full)
                {
                    let i = mesh.vertex.len() as u32;
                    mesh.vertex.extend([
                        Vec3::new((x - sx + 1) as _, (y - sy) as _, (z - sz) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy + 1) as _, (z - sz) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                    ]);
                    mesh.normal.extend(iter::repeat(Vec3::X).take(4));
                    mesh.tangent
                        .extend(iter::repeat(Vec3::NEG_Z.extend(1.)).take(4));
                    mesh.uv.extend([uv2, uv1, uv4, uv3]);
                    mesh.index.extend([i, i + 1, i + 3, i, i + 3, i + 2]);
                }

                // Right
                if (x == 0) || (block_type((data[(x - 1, y, z)] & 0xff) as u8) != BlockType::Full) {
                    let i = mesh.vertex.len() as u32;
                    mesh.vertex.extend([
                        Vec3::new((x - sx) as _, (y - sy) as _, (z - sz) as _),
                        Vec3::new((x - sx) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx) as _, (y - sy + 1) as _, (z - sz) as _),
                        Vec3::new((x - sx) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                    ]);
                    mesh.normal.extend(iter::repeat(Vec3::NEG_X).take(4));
                    mesh.tangent
                        .extend(iter::repeat(Vec3::Z.extend(1.)).take(4));
                    mesh.uv.extend([uv1, uv2, uv3, uv4]);
                    mesh.index.extend([i + 1, i, i + 2, i + 1, i + 2, i + 3]);
                }

                // Back
                if (z + 1 >= ez)
                    || (block_type((data[(x, y, z + 1)] & 0xff) as u8) != BlockType::Full)
                {
                    let i = mesh.vertex.len() as u32;
                    mesh.vertex.extend([
                        Vec3::new((x - sx) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                    ]);
                    mesh.normal.extend(iter::repeat(Vec3::Z).take(4));
                    mesh.tangent
                        .extend(iter::repeat(Vec3::X.extend(1.)).take(4));
                    mesh.uv.extend([uv1, uv2, uv3, uv4]);
                    mesh.index.extend([i + 1, i, i + 2, i + 1, i + 2, i + 3]);
                }

                // Front
                if (z == 0) || (block_type((data[(x, y, z - 1)] & 0xff) as u8) != BlockType::Full) {
                    let i = mesh.vertex.len() as u32;
                    mesh.vertex.extend([
                        Vec3::new((x - sx) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                        Vec3::new((x - sx + 1) as _, (y - sy + 1) as _, (z - sz + 1) as _),
                    ]);
                    mesh.normal.extend(iter::repeat(Vec3::NEG_Z).take(4));
                    mesh.tangent
                        .extend(iter::repeat(Vec3::NEG_X.extend(1.)).take(4));
                    mesh.uv.extend([uv2, uv1, uv4, uv3]);
                    mesh.index.extend([i, i + 1, i + 3, i, i + 3, i + 2]);
                }
            }
        }
    };
    for x in sx..ex {
        for y in sy..ey {
            for z in sz..ez {
                f(x, y, z);
            }
        }
    }
}
