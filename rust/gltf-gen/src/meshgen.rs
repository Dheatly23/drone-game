// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::cmp::Ordering;
use std::iter;

use anyhow::{bail, Error};
use either::Either;
use linked_hash_map::LinkedHashMap;
use nalgebra::{
    convert, convert_unchecked, Affine3, Isometry3, Matrix2, Matrix2x4, Matrix3x2, Matrix3x4,
    Matrix4, Vector2, Vector3, Vector4,
};
use ndarray::{azip, s, Array2, ArrayView, ArrayView2, ArrayViewMut, ArrayViewMut2, Axis};
use num_traits::ToBytes;

use super::{gltf, parse};

fn process_trs(data: &parse::TransformTRS) -> Affine3<f32> {
    let mut ret: Affine3<f32> = convert(Isometry3::from_parts(data.translation, data.rotation));
    ret.matrix_mut_unchecked()
        .prepend_nonuniform_scaling_mut(&data.scale);
    ret
}

fn process_transform(transform: &parse::Transform) -> Affine3<f32> {
    match transform {
        parse::Transform::Identity => <Affine3<f32>>::identity(),
        parse::Transform::Matrix { matrix } => {
            let mut m = matrix.insert_row(3, 0.0);
            m[(3, 3)] = 1.0;
            <Affine3<f32>>::from_matrix_unchecked(m)
        }
        parse::Transform::Trs { data } => process_trs(data),
    }
}

fn weight_normalize(mut weights: Vector4<f32>) -> Vector4<u16> {
    let s = weights.sum();
    if s < 1e-7 {
        return Vector4::new(u16::MAX, 0, 0, 0);
    }
    weights *= 65535. / s;
    weights.iter_mut().for_each(|v| *v = v.round());

    let mut weights: Vector4<u16> = convert_unchecked(weights);
    // Renormalize possible residue
    let ix = weights.imax();
    let mut s: i32 = weights.iter().map(|&v| v as i32).sum();
    s -= u16::MAX as i32;
    match s.cmp(&0) {
        Ordering::Equal => (),
        Ordering::Greater => weights[ix] -= (-s) as u16,
        Ordering::Less => weights[ix] += s as u16,
    }

    weights
}

fn plane_flags(
    parse::PlaneData {
        tangent, color, uv, ..
    }: &parse::PlaneData,
) -> parse::AttrFlags {
    parse::AttrFlags {
        normal: true,
        tangent: tangent.is_some(),
        uv: uv.is_some(),
        color: color.is_some(),
        joints: false,
        weights: false,
        index: true,
    }
}

fn joint_flags() -> parse::AttrFlags {
    parse::AttrFlags {
        normal: false,
        tangent: false,
        uv: false,
        color: false,
        joints: true,
        weights: true,
        index: false,
    }
}

fn plane_params(
    (name, ix): (&str, usize),
    flags: &parse::AttrFlags,
    transform: &Affine3<f32>,
    plane: &parse::PlaneData,
) -> Result<([Vector2<f32>; 4], Vector3<f32>, Option<Vector4<f32>>), Error> {
    let parse::PlaneData {
        p1,
        p2,
        p3,
        p4,
        flip,
        normal,
        tangent,
        uv,
        duv,
        uv_swap,
        ..
    } = plane;
    let (uv1, uv2, uv3, uv4) = match (uv, duv) {
        (Some(uv), Some(duv)) => {
            let end = *uv + *duv;
            if *uv_swap {
                (
                    *uv,
                    Vector2::new(uv.x, end.y),
                    Vector2::new(end.x, uv.y),
                    end,
                )
            } else {
                (
                    *uv,
                    Vector2::new(end.x, uv.y),
                    Vector2::new(uv.x, end.y),
                    end,
                )
            }
        }
        (None, None) if *uv_swap => (
            Vector2::new(0.0, 0.0),
            Vector2::new(0.0, 1.0),
            Vector2::new(1.0, 0.0),
            Vector2::new(1.0, 1.0),
        ),
        (None, None) => (
            Vector2::new(0.0, 0.0),
            Vector2::new(1.0, 0.0),
            Vector2::new(0.0, 1.0),
            Vector2::new(1.0, 1.0),
        ),
        _ => bail!("Error at mesh {name} command {ix}: UV not all specified"),
    };
    let dv1 = p4 - p1;
    let dv2 = p3 - p2;

    let mut normal = match normal {
        Some(v) => *v,
        None if *flip => dv2.cross(&dv1),
        None => dv1.cross(&dv2),
    };

    let tangent = if flags.tangent {
        let mut t = if let Some(v) = tangent {
            *v
        } else {
            let mut m = Matrix2::from_columns(&[uv3 - uv2, uv4 - uv1]);
            if !m.try_inverse_mut() {
                bail!("Error at mesh {name} command {ix}: UV matrix {m} is non-invertible");
            }
            let m = Matrix3x2::from_columns(&[dv2, dv1]) * m;

            let w = if m
                .column(1)
                .dot(&m.column(0).cross(&normal))
                .is_sign_negative()
                ^ *flip
            {
                1.0
            } else {
                -1.0
            };
            m.column(0).insert_row(3, w)
        };
        let mut t_ = t.fixed_rows_mut::<3>(0);
        t_.set_column(0, &transform.transform_vector(&t_.xyz()));
        t_.normalize_mut();
        Some(t)
    } else {
        None
    };

    normal = transform.transform_vector(&normal);
    normal.normalize_mut();

    Ok(([uv1, uv2, uv3, uv4], normal, tangent))
}

fn joints_param(
    flags: &parse::AttrFlags,
    joint: &Option<parse::JointData>,
) -> (Vector4<u16>, Vector4<u16>) {
    if let Some(j) = joint {
        assert!(flags.joints);
        assert!(flags.weights);
        (j.joints.into(), weight_normalize(j.weights.abs()))
    } else {
        (Vector4::zeros(), Vector4::new(u16::MAX, 0, 0, 0))
    }
}

fn process_grid_plane(
    flip: bool,
    grid: ArrayView2<u8>,
    mask: u8,
    mut aux: ArrayViewMut2<(usize, usize, usize)>,
    indices: &mut Vec<usize>,
    points: &mut LinkedHashMap<(usize, usize), usize>,
    count: &mut usize,
) -> Result<(), Error> {
    // Find largest rectangle
    let mut pr: Option<(ArrayViewMut<_, _>, ArrayView<_, _>)> = None;
    for (vr, mut ar) in grid.outer_iter().zip(aux.outer_iter_mut()) {
        let mut p = 0;

        // Update right expand
        let mut ar_ = ar.view_mut();
        ar_.invert_axis(Axis(0));
        for ((c, a), v) in ar_.indexed_iter_mut().zip(vr.into_iter()) {
            if v & mask == 0 {
                *a = (0, 0, 0);
                p = c + 1;
                continue;
            }

            a.2 = c - p + 1;
        }

        // Update expands
        p = 0;
        for (((c, (l, u, r)), v), ((pl, pu, pr), pv)) in ar
            .indexed_iter_mut()
            .zip(vr.into_iter().copied())
            .zip(match pr {
                Some((a, v)) => Either::Left(a.into_iter().zip(v).map(|(a, v)| (*a, *v))),
                None => Either::Right(iter::repeat_with(<_>::default)),
            })
        {
            if v & mask == 0 {
                p = c + 1;
                continue;
            }

            *u = pu + 1;
            *l = c - p + 1;
            if pv & mask != 0 {
                *l = (*l).min(pl);
                *r = (*r).min(pr);
            }
        }
        pr = Some((ar, vr));
    }

    let mut cache = |x, y| {
        let l = points.len();
        *points.entry((x, y)).or_insert(l) + *count
    };

    // Cut largest rectangle iteratively
    while let Some(((y, x), &(mut l, mut u, r), _)) =
        aux.indexed_iter().fold(None, |a, (b, bv @ &(l, u, r))| {
            let sb = (l + r - 1) * u;
            match a {
                Some((_, _, sa)) if sa >= sb => a,
                _ if sb == 0 => None,
                _ => Some((b, bv, sb)),
            }
        })
    {
        l -= 1;
        u -= 1;
        let p2 = cache(x - l, y - u);
        let p1 = cache(x - l, y + 1);
        let p4 = cache(x + r, y - u);
        let p3 = cache(x + r, y + 1);

        indices.extend(if flip {
            [p1, p3, p2, p4, p2, p3]
        } else {
            [p1, p2, p4, p1, p4, p3]
        });

        // Remove rectangle and update sizes
        aux.slice_mut(s![y - u..y + 1, x - l..x + r])
            .fill((0, 0, 0));
        if y + 1 != aux.raw_dim()[0] {
            for (c, mut a) in aux
                .slice_mut(s![y + 1.., ..x - l;-1])
                .axis_iter_mut(Axis(0))
                .enumerate()
            {
                for (r, (_, u, r_)) in a.indexed_iter_mut() {
                    if *u <= c + 1 {
                        break;
                    }
                    *r_ = (*r_).min(r + 1);
                }
            }
            for mut a in aux.slice_mut(s![y + 1.., x - l..]).axis_iter_mut(Axis(1)) {
                for (r, (_, u, _)) in a.indexed_iter_mut() {
                    if *u == 0 {
                        break;
                    }
                    *u = (*u).min(r + 1);
                }
            }
        }
        for mut a in aux
            .slice_mut(s![y - u..y + 1, ..x - l;-1])
            .axis_iter_mut(Axis(0))
        {
            for (c, (_, _, r)) in a.indexed_iter_mut() {
                if *r == 0 {
                    break;
                }
                *r = (*r).min(c + 1);
            }
        }
        if x + r != aux.raw_dim()[1] {
            for mut a in aux.slice_mut(s![y - u.., x + r..]).axis_iter_mut(Axis(0)) {
                for (c, (l, _, _)) in a.indexed_iter_mut() {
                    if *l == 0 {
                        break;
                    }
                    *l = (*l).min(c + 1);
                }
            }
        }
    }
    *count += points.len();

    Ok(())
}

fn process_grid_cells(
    mut out: ArrayViewMut2<u8>,
    p: Option<ArrayView2<u8>>,
    a: Option<ArrayView2<u8>>,
) -> bool {
    enum CellType {
        Empty,
        Fill,
        NoBorder,
    }

    impl From<u8> for CellType {
        fn from(v: u8) -> Self {
            if v & 2 != 0 {
                Self::NoBorder
            } else if v != 0 {
                Self::Fill
            } else {
                Self::Empty
            }
        }
    }

    match (p, a) {
        (None, None) => return true,
        (None, Some(a)) => out.zip_mut_with(&a, |t, &v| {
            *t = if let CellType::Fill = CellType::from(v) {
                2
            } else {
                0
            }
        }),
        (Some(p), None) => out.zip_mut_with(&p, |t, &v| {
            *t = if let CellType::Fill = CellType::from(v) {
                1
            } else {
                0
            }
        }),
        (Some(p), Some(a)) => azip![(t in &mut out, &p in p, &a in a) {
            *t = match (CellType::from(p), CellType::from(a)) {
                (CellType::NoBorder, _) | (_, CellType::NoBorder) | (CellType::Empty, CellType::Empty) | (CellType::Fill, CellType::Fill) => 0,
                (CellType::Empty, CellType::Fill) => 2,
                (CellType::Fill, CellType::Empty) => 1,
            }
        }],
    }
    false
}

fn to_bytes<'a, I, T>(it: I) -> impl Iterator<Item = u8> + 'a
where
    I: 'a + IntoIterator<Item = &'a T>,
    T: 'a + ToBytes,
    T::Bytes: IntoIterator<Item = u8>,
{
    it.into_iter().flat_map(|v| v.to_le_bytes())
}

fn add_short_indices<I>(buffer: &mut Vec<u8>, it: I)
where
    I: IntoIterator<Item = u16>,
{
    buffer.extend(it.into_iter().flat_map(|v| v.to_le_bytes()));
    buffer.resize((buffer.len() + 3) & !3, 0);
}

pub fn generate_mesh(
    mesh_name: &str,
    mesh: &parse::Mesh,
    gltf: &mut gltf::Gltf,
    buffer: &mut Vec<u8>,
) -> Result<gltf::MeshPrimitive, Error> {
    let transform = process_transform(&mesh.transform);

    let mut flags = mesh.data.iter().fold(mesh.generate, |a, b| {
        a | match b {
            parse::MeshData::Triangles {
                normal,
                tangent,
                uv,
                color,
                joints,
                weights,
                index,
                ..
            } => parse::AttrFlags {
                normal: !normal.is_empty(),
                tangent: !tangent.is_empty(),
                uv: !uv.is_empty(),
                color: !color.is_empty(),
                joints: !joints.is_empty(),
                weights: !weights.is_empty(),
                index: !index.is_empty(),
            },
            parse::MeshData::Plane { plane, joint, .. }
            | parse::MeshData::GridPlaneSimple { plane, joint, .. } => {
                plane_flags(plane)
                    | match joint {
                        Some(_) => joint_flags(),
                        None => parse::AttrFlags::default(),
                    }
            }
            parse::MeshData::PlaneJoint { plane, .. } => plane_flags(plane) | joint_flags(),
            parse::MeshData::GridPlaneJointed { plane, .. } => plane_flags(plane) | joint_flags(),
            parse::MeshData::VoxelSimple { color, joint, .. } => parse::AttrFlags {
                normal: true,
                tangent: false,
                uv: false,
                color: color.is_some(),
                joints: joint.is_some(),
                weights: joint.is_some(),
                index: true,
            },
        }
    });
    flags.tangent &= flags.normal;
    flags.weights &= flags.joints;

    let mut data_index = Vec::new();
    let has_blend = !mesh.blend.is_empty();

    const POSITION_SIZE: usize = 4 * 3;
    let normal_offset = POSITION_SIZE;
    const NORMAL_SIZE: usize = 4 * 3;
    let tangent_offset = normal_offset + if flags.normal { NORMAL_SIZE } else { 0 };
    const TANGENT_SIZE: usize = 4 * 4;
    let uv_offset = tangent_offset + if flags.tangent { TANGENT_SIZE } else { 0 };
    const UV_SIZE: usize = 4 * 2;
    let color_offset = uv_offset + if flags.uv { UV_SIZE } else { 0 };
    const COLOR_SIZE: usize = 4;
    let joints_offset = color_offset + if flags.color { COLOR_SIZE } else { 0 };
    const JOINTS_SIZE: usize = 2 * 4;
    let weights_offset = joints_offset + if flags.joints { JOINTS_SIZE } else { 0 };
    const WEIGHTS_SIZE: usize = 2 * 4;
    let total_size = weights_offset + if flags.weights { WEIGHTS_SIZE } else { 0 };

    let mut view = gltf::BufferView {
        buffer: 0,
        byte_offset: buffer.len(),
        byte_length: 0,
        byte_stride: total_size,
    };
    let mut indices = Vec::new();
    let mut count = 0;

    for (ix, i) in mesh.data.iter().enumerate() {
        if has_blend {
            data_index.push(count);
        }

        if let parse::MeshData::Triangles {
            position,
            normal,
            tangent,
            uv,
            color,
            joints,
            weights,
            index,
        } = i
        {
            for (i, &pos) in position.iter().enumerate() {
                buffer.extend(to_bytes(&transform.transform_point(&pos.into()).coords));

                if flags.normal {
                    let mut normal = match normal.get(i).or(normal.last()) {
                        Some(v) => transform.transform_vector(v),
                        None => transform.matrix().column(1).xyz(),
                    };
                    normal.normalize_mut();
                    buffer.extend(to_bytes(&normal));
                }

                if flags.tangent {
                    let mut tangent = match tangent.get(i).or(tangent.last()) {
                        Some(v) => transform
                            .transform_vector(&v.xyz())
                            .xyz()
                            .insert_row(3, if v.w.is_sign_positive() { 1.0 } else { -1.0 }),
                        None => transform.matrix().column(0).xyz().insert_row(3, 1.0),
                    };
                    tangent.fixed_rows_mut::<3>(0).normalize_mut();
                    buffer.extend(to_bytes(&tangent));
                }

                if flags.uv {
                    buffer.extend(to_bytes(
                        &uv.get(i).or(uv.last()).copied().unwrap_or_default(),
                    ));
                }

                if flags.color {
                    buffer.extend(match color.get(i).or(color.last()) {
                        Some(&v) => v,
                        None => [u8::MAX; 4],
                    });
                }

                if flags.joints {
                    buffer.extend(to_bytes(
                        &joints.get(i).or(joints.last()).copied().unwrap_or_default(),
                    ));
                }

                if flags.weights {
                    let w = match weights.get(i).or(weights.last()) {
                        Some(v) => weight_normalize(v.abs()),
                        None => Vector4::new(u16::MAX, 0, 0, 0),
                    };
                    buffer.extend(to_bytes(&w));
                }
            }

            if flags.index {
                if index.is_empty() {
                    indices.extend(count..count + position.len());
                } else {
                    indices.extend(index.iter().map(|&v| v + count));
                }
            }
            count += position.len();
        } else if let parse::MeshData::Plane { trs, plane, joint } = i {
            let transform = transform * process_trs(trs);

            let ([uv1, uv2, uv3, uv4], normal, tangent) =
                plane_params((mesh_name, ix), &flags, &transform, plane)?;
            let parse::PlaneData {
                p1,
                p2,
                p3,
                p4,
                color,
                flip,
                ..
            } = plane;
            let color = color.unwrap_or([u8::MAX; 4]);
            let (joints, weights) = joints_param(&flags, joint);

            assert!(flags.normal);
            for (p, uv) in [(p1, uv1), (p2, uv2), (p3, uv3), (p4, uv4)] {
                buffer.extend(to_bytes(&transform.transform_point(&(*p).into()).coords));

                buffer.extend(to_bytes(&normal));
                if let Some(t) = &tangent {
                    buffer.extend(to_bytes(t));
                }
                if flags.uv {
                    buffer.extend(to_bytes(&uv));
                }
                if flags.color {
                    buffer.extend(color);
                }
                if flags.joints {
                    buffer.extend(to_bytes(&joints));
                }
                if flags.weights {
                    buffer.extend(to_bytes(&weights));
                }
            }

            assert!(flags.index);
            indices.extend(
                if *flip {
                    [0, 2, 1, 3, 1, 2]
                } else {
                    [0, 1, 3, 0, 3, 2]
                }
                .into_iter()
                .map(|v| v + count),
            );
            count += 4;
        } else if let parse::MeshData::PlaneJoint {
            trs,
            plane,
            j1,
            w1,
            j2,
            w2,
            j3,
            w3,
            j4,
            w4,
        } = i
        {
            let transform = transform * process_trs(trs);

            let ([uv1, uv2, uv3, uv4], normal, tangent) =
                plane_params((mesh_name, ix), &flags, &transform, plane)?;
            let parse::PlaneData {
                p1,
                p2,
                p3,
                p4,
                color,
                flip,
                ..
            } = plane;
            let color = color.unwrap_or([u8::MAX; 4]);

            assert!(flags.normal);
            assert!(flags.joints);
            assert!(flags.weights);
            for (p, uv, j, w) in [
                (p1, uv1, j1, w1),
                (p2, uv2, j2, w2),
                (p3, uv3, j3, w3),
                (p4, uv4, j4, w4),
            ] {
                buffer.extend(to_bytes(&transform.transform_point(&(*p).into()).coords));

                buffer.extend(to_bytes(&normal));
                if let Some(t) = &tangent {
                    buffer.extend(to_bytes(t));
                }
                if flags.uv {
                    buffer.extend(to_bytes(&uv));
                }
                if flags.color {
                    buffer.extend(color);
                }
                buffer.extend(to_bytes(j));
                buffer.extend(to_bytes(&weight_normalize(w.abs())));
            }

            assert!(flags.index);
            indices.extend(
                if *flip {
                    [0, 2, 1, 3, 1, 2]
                } else {
                    [0, 1, 3, 0, 3, 2]
                }
                .into_iter()
                .map(|v| v + count),
            );
            count += 4;
        } else if let parse::MeshData::GridPlaneSimple {
            trs,
            plane,
            joint,
            size,
            grid,
        } = i
        {
            let grid = ArrayView::from_shape(*size, grid)?;
            if !grid.iter().any(|&v| v != 0) {
                continue;
            }

            let transform = transform * process_trs(trs);

            let (uv, normal, tangent) = plane_params((mesh_name, ix), &flags, &transform, plane)?;

            let mut points = LinkedHashMap::new();
            let mut aux = <Array2<(usize, usize, usize)>>::default(*size);
            process_grid_plane(
                plane.flip,
                grid,
                u8::MAX,
                aux.view_mut(),
                &mut indices,
                &mut points,
                &mut count,
            )?;

            // Add all the points
            let parse::PlaneData {
                p1,
                p2,
                p3,
                p4,
                color,
                ..
            } = plane;
            let color = color.unwrap_or([u8::MAX; 4]);
            let (joints, weights) = joints_param(&flags, joint);
            let m = (transform.matrix()
                * Matrix3x4::from_columns(&[*p1, *p2, *p3, *p4]).insert_row(3, 1.))
            .remove_row(3);
            let muv = if flags.uv {
                Some(Matrix2x4::from_columns(&uv))
            } else {
                None
            };
            let mut pv = 0;
            let ey = (aux.raw_dim()[0] as f32).recip();
            let ex = (aux.raw_dim()[1] as f32).recip();

            assert!(flags.normal);
            for ((x, y), v) in points {
                assert_eq!(pv, v);
                pv = v + 1;

                let dy = y as f32 * ey;
                let dx = x as f32 * ex;
                let dv = Vector4::new(
                    (1. - dx) * (1. - dy),
                    dx * (1. - dy),
                    (1. - dx) * dy,
                    dx * dy,
                );
                let p = m * dv;
                buffer.extend(to_bytes(&p));

                buffer.extend(to_bytes(&normal));
                if let Some(t) = &tangent {
                    buffer.extend(to_bytes(t));
                }
                if let Some(muv) = &muv {
                    let uv = muv * dv;
                    buffer.extend(to_bytes(&uv));
                }
                if flags.color {
                    buffer.extend(color);
                }
                if flags.joints {
                    buffer.extend(to_bytes(&joints));
                }
                if flags.weights {
                    buffer.extend(to_bytes(&weights));
                }
            }
        } else if let parse::MeshData::GridPlaneJointed {
            trs,
            plane,
            joints,
            w1,
            w2,
            w3,
            w4,
            mesh_type,
            size,
            grid,
        } = i
        {
            let grid = ArrayView::from_shape(*size, grid)?;
            if !grid.iter().any(|&v| v != 0) {
                continue;
            }

            let transform = transform * process_trs(trs);

            let (uv, normal, tangent) = plane_params((mesh_name, ix), &flags, &transform, plane)?;
            let parse::PlaneData {
                p1,
                p2,
                p3,
                p4,
                color,
                flip,
                ..
            } = plane;
            let color = color.unwrap_or([u8::MAX; 4]);

            let mut points = Vec::new();

            let mut prev: Option<ArrayView<_, _>> = None;
            let ey = grid.raw_dim()[1] * 2;
            for (mut i, a) in grid
                .outer_iter()
                .map(Some)
                .chain(iter::repeat_with(|| None))
                .enumerate()
            {
                if a.is_none() && prev.is_none() {
                    break;
                }
                i *= 2;

                if !matches!(
                    (
                        a.as_ref().and_then(|v| v.last()),
                        prev.as_ref().and_then(|v| v.last()),
                    ),
                    (None | Some(0), None | Some(0))
                ) {
                    points.push((i, ey));
                }

                points.extend(
                    match &a {
                        Some(v) => Either::Left(v.indexed_iter().map(|(i, &v)| (i, v))),
                        None => Either::Right(iter::repeat(0).enumerate()),
                    }
                    .zip(match prev {
                        Some(v) => Either::Left(v.into_iter().copied()),
                        None => Either::Right(iter::repeat(0)),
                    })
                    .filter_map(|((j, a), p)| match (a, p) {
                        (0, 0) => None,
                        _ => Some((i, j * 2)),
                    }),
                );
                prev = a;
            }
            if let parse::GridPlaneMeshType::QuadCenter = mesh_type {
                points.extend(grid.indexed_iter().filter_map(|((i, j), &v)| match v {
                    0 => None,
                    _ => Some((i * 2 + 1, j * 2 + 1)),
                }));
            }

            points.sort_unstable();
            points.dedup();

            let get_point = |p: &[_], k| match p.binary_search(&k) {
                Ok(v) => v,
                Err(_) => unreachable!("Point {k:?} does not exist!"),
            };
            for ((mut i, mut j), &v) in grid.indexed_iter() {
                if v == 0 {
                    continue;
                }
                i *= 2;
                j *= 2;

                let p1 = get_point(&points, (i, j));
                let p2 = get_point(&points[p1..], (i, j + 2));
                let p3 = get_point(&points[p2..], (i + 2, j));
                let p4 = get_point(&points[p3..], (i + 2, j + 2));
                if let parse::GridPlaneMeshType::Quad = mesh_type {
                    indices.extend(
                        if *flip {
                            [p1, p3, p2, p4, p2, p3]
                        } else {
                            [p1, p2, p4, p1, p4, p3]
                        }
                        .into_iter()
                        .map(|v| v + count),
                    );
                } else if let parse::GridPlaneMeshType::QuadCenter = mesh_type {
                    let p5 = get_point(&points[p2..], (i + 1, j + 1));

                    indices.extend(
                        if *flip {
                            [p1, p5, p2, p2, p5, p4, p4, p5, p3, p3, p5, p1]
                        } else {
                            [p1, p2, p5, p2, p4, p5, p4, p3, p5, p3, p1, p5]
                        }
                        .into_iter()
                        .map(|v| v + count),
                    );
                }
            }

            // Add points
            count += points.len();
            let ey = 2.0 / (grid.raw_dim()[0] as f32);
            let ex = 2.0 / (grid.raw_dim()[1] as f32);
            let m = (transform.matrix()
                * Matrix3x4::from_columns(&[*p1, *p2, *p3, *p4]).insert_row(3, 1.))
            .remove_row(3);
            let muv = if flags.uv {
                Some(Matrix2x4::from_columns(&uv))
            } else {
                None
            };
            let mw = Matrix4::from_columns(&[*w1, *w2, *w3, *w4]);

            assert!(flags.normal);
            assert!(flags.joints);
            assert!(flags.weights);
            for (y, x) in points {
                let dy = y as f32 * ey;
                let dx = x as f32 * ex;
                let dv = Vector4::new(
                    (1. - dx) * (1. - dy),
                    dx * (1. - dy),
                    (1. - dx) * dy,
                    dx * dy,
                );
                let p = m * dv;
                buffer.extend(to_bytes(&p));

                buffer.extend(to_bytes(&normal));
                if let Some(t) = &tangent {
                    buffer.extend(to_bytes(t));
                }
                if let Some(muv) = &muv {
                    let uv = muv * dv;
                    buffer.extend(to_bytes(&uv));
                }
                if flags.color {
                    buffer.extend(color);
                }
                buffer.extend(to_bytes(joints));
                buffer.extend(to_bytes(&weight_normalize((mw * dv).abs())));
            }
        } else if let parse::MeshData::VoxelSimple {
            trs,
            p1,
            p2,
            p3,
            p4,
            p5,
            p6,
            p7,
            p8,
            joint,
            color,
            size,
            grid,
        } = i
        {
            assert!(flags.normal);
            let mut grid = ArrayView::from_shape([size[1], size[0], size[2]], grid)?;
            if !grid.iter().any(|&v| v != 0) {
                continue;
            }
            grid.invert_axis(Axis(0));

            let transform = transform * process_trs(trs);
            let color = color.unwrap_or([u8::MAX; 4]);
            let (joints, weights) = joints_param(&flags, joint);

            let mut aux: Array2<(usize, usize, usize)>;
            let mut temp: Array2<u8>;
            let mut points = LinkedHashMap::new();

            let ex = (size[0] as f32).recip();
            let ey = (size[1] as f32).recip();
            let ez = (size[2] as f32).recip();

            let mut f = |points: &mut LinkedHashMap<(usize, usize), usize>,
                         m: &Matrix3x4<f32>,
                         normal: &Vector3<f32>,
                         tangent: &Option<Vector4<f32>>,
                         ex: f32,
                         ey: f32| {
                // Add points
                let mut pv = 0;
                for ((x, y), v) in points.drain() {
                    assert_eq!(pv, v);
                    pv = v + 1;

                    let dy = y as f32 * ey;
                    let dx = x as f32 * ex;
                    let dv = Vector4::new(
                        (1. - dx) * (1. - dy),
                        dx * (1. - dy),
                        (1. - dx) * dy,
                        dx * dy,
                    );
                    let p = m * dv;
                    buffer.extend(to_bytes(&p));

                    buffer.extend(to_bytes(normal));
                    if let Some(t) = tangent {
                        buffer.extend(to_bytes(t));
                    }
                    if flags.uv {
                        buffer.extend(to_bytes(&Vector2::new(0.0, 0.0)));
                    }
                    if flags.color {
                        buffer.extend(color);
                    }
                    if flags.joints {
                        buffer.extend(to_bytes(&joints));
                    }
                    if flags.weights {
                        buffer.extend(to_bytes(&weights));
                    }
                }
            };
            let g = |p1: &Vector3<f32>,
                     p2: &Vector3<f32>,
                     p3: &Vector3<f32>,
                     p4: &Vector3<f32>|
             -> (Vector3<f32>, Option<Vector4<f32>>) {
                let mut normal = (p4 - p1).cross(&(p3 - p2));
                let tangent = if flags.tangent {
                    let mut tangent = p2 - p1;
                    tangent += p4;
                    tangent -= p3;
                    let mut bitangent = p3 - p1;
                    bitangent += p4;
                    bitangent -= p2;
                    let w = if normal.cross(&tangent).dot(&bitangent).is_sign_positive() {
                        1.0
                    } else {
                        -1.0
                    };
                    tangent = transform.transform_vector(&tangent);
                    tangent.normalize_mut();
                    Some(tangent.insert_row(3, w))
                } else {
                    None
                };
                normal = transform.transform_vector(&normal);
                normal.normalize_mut();
                (normal, tangent)
            };

            // Up/Down
            {
                temp = Array2::default((size[0], size[2]));
                aux = Array2::default(temp.raw_dim());
                let mut prev = None;
                for (i, a) in grid
                    .outer_iter()
                    .map(Some)
                    .chain(iter::repeat_with(|| None))
                    .enumerate()
                {
                    if process_grid_cells(temp.view_mut(), prev, a) {
                        break;
                    }
                    prev = a;

                    let v_ = (i as f32) * ey;
                    let iv = 1. - v_;
                    let p1 = p1 * iv + p5 * v_;
                    let p2 = p2 * iv + p6 * v_;
                    let p3 = p3 * iv + p7 * v_;
                    let p4 = p4 * iv + p8 * v_;
                    let (mut normal, mut tangent) = g(&p1, &p2, &p3, &p4);
                    let m = (transform.matrix()
                        * Matrix3x4::from_columns(&[p1, p2, p3, p4]).insert_row(3, 1.0))
                    .remove_row(3);

                    // Up
                    process_grid_plane(
                        false,
                        temp.view(),
                        1,
                        aux.view_mut(),
                        &mut indices,
                        &mut points,
                        &mut count,
                    )?;
                    f(&mut points, &m, &normal, &tangent, ex, ez);

                    // Down
                    normal.neg_mut();
                    if let Some(t) = &mut tangent {
                        t.w = -t.w;
                    }
                    process_grid_plane(
                        true,
                        temp.view(),
                        2,
                        aux.view_mut(),
                        &mut indices,
                        &mut points,
                        &mut count,
                    )?;
                    f(&mut points, &m, &normal, &tangent, ex, ez);
                }
            }

            // Left/Right
            {
                temp = Array2::default((size[1], size[2]));
                aux = Array2::default(temp.raw_dim());
                let mut prev = None;
                for (i, a) in grid
                    .axis_iter(Axis(1))
                    .map(Some)
                    .chain(iter::repeat_with(|| None))
                    .enumerate()
                {
                    if process_grid_cells(temp.view_mut(), prev, a) {
                        break;
                    }
                    prev = a;

                    let v_ = (i as f32) * ex;
                    let iv = 1. - v_;
                    let p1 = p1 * iv + p3 * v_;
                    let p2 = p2 * iv + p4 * v_;
                    let p3 = p5 * iv + p7 * v_;
                    let p4 = p6 * iv + p8 * v_;
                    let (mut normal, mut tangent) = g(&p1, &p2, &p3, &p4);
                    let m = (transform.matrix()
                        * Matrix3x4::from_columns(&[p1, p2, p3, p4]).insert_row(3, 1.0))
                    .remove_row(3);

                    // Left
                    process_grid_plane(
                        false,
                        temp.view(),
                        2,
                        aux.view_mut(),
                        &mut indices,
                        &mut points,
                        &mut count,
                    )?;
                    f(&mut points, &m, &normal, &tangent, ez, ey);

                    // Right
                    normal.neg_mut();
                    if let Some(t) = &mut tangent {
                        t.w = -t.w;
                    }
                    process_grid_plane(
                        true,
                        temp.view(),
                        1,
                        aux.view_mut(),
                        &mut indices,
                        &mut points,
                        &mut count,
                    )?;
                    f(&mut points, &m, &normal, &tangent, ez, ey);
                }
            }

            // Front/Back
            {
                temp = Array2::default((size[1], size[0]));
                aux = Array2::default(temp.raw_dim());
                let mut prev = None;
                for (i, a) in grid
                    .axis_iter(Axis(2))
                    .map(Some)
                    .chain(iter::repeat_with(|| None))
                    .enumerate()
                {
                    if process_grid_cells(temp.view_mut(), prev, a) {
                        break;
                    }
                    prev = a;

                    let v_ = (i as f32) * ez;
                    let iv = 1. - v_;
                    let p1 = p1 * iv + p2 * v_;
                    let p2 = p3 * iv + p4 * v_;
                    let p3 = p5 * iv + p6 * v_;
                    let p4 = p7 * iv + p8 * v_;
                    let (mut normal, mut tangent) = g(&p1, &p2, &p3, &p4);
                    let m = (transform.matrix()
                        * Matrix3x4::from_columns(&[p1, p2, p3, p4]).insert_row(3, 1.0))
                    .remove_row(3);

                    // Back
                    process_grid_plane(
                        false,
                        temp.view(),
                        1,
                        aux.view_mut(),
                        &mut indices,
                        &mut points,
                        &mut count,
                    )?;
                    f(&mut points, &m, &normal, &tangent, ex, ey);

                    // Front
                    normal.neg_mut();
                    if let Some(t) = &mut tangent {
                        t.w = -t.w;
                    }
                    process_grid_plane(
                        true,
                        temp.view(),
                        2,
                        aux.view_mut(),
                        &mut indices,
                        &mut points,
                        &mut count,
                    )?;
                    f(&mut points, &m, &normal, &tangent, ex, ey);
                }
            }
        }
    }

    view.byte_length = count * total_size;

    let gltf::Gltf {
        buffer_views,
        accessors,
        ..
    } = gltf;

    let mut ret = gltf::MeshPrimitive {
        attributes: gltf::MeshAttribute {
            position: Some(accessors.len()),
            normal: None,
            tangent: None,
            texcoord_0: None,
            color_0: None,
            joints_0: None,
            weights_0: None,
        },
        targets: Vec::new(),
        indices: None,
        material: None,
    };

    accessors.push(gltf::Accessor {
        buffer_view: Some(buffer_views.len()),
        byte_offset: 0,
        count,
        component_type: gltf::ComponentType::FLOAT,
        type_: gltf::AccessorType::VEC3,
        normalized: false,
        sparse: None,
    });
    if flags.normal {
        ret.attributes.normal = Some(accessors.len());
        accessors.push(gltf::Accessor {
            buffer_view: Some(buffer_views.len()),
            byte_offset: normal_offset,
            count,
            component_type: gltf::ComponentType::FLOAT,
            type_: gltf::AccessorType::VEC3,
            normalized: false,
            sparse: None,
        });
    }
    if flags.tangent {
        ret.attributes.tangent = Some(accessors.len());
        accessors.push(gltf::Accessor {
            buffer_view: Some(buffer_views.len()),
            byte_offset: tangent_offset,
            count,
            component_type: gltf::ComponentType::FLOAT,
            type_: gltf::AccessorType::VEC4,
            normalized: false,
            sparse: None,
        });
    }
    if flags.uv {
        ret.attributes.texcoord_0 = Some(accessors.len());
        accessors.push(gltf::Accessor {
            buffer_view: Some(buffer_views.len()),
            byte_offset: uv_offset,
            count,
            component_type: gltf::ComponentType::FLOAT,
            type_: gltf::AccessorType::VEC2,
            normalized: false,
            sparse: None,
        });
    }
    if flags.color {
        ret.attributes.color_0 = Some(accessors.len());
        accessors.push(gltf::Accessor {
            buffer_view: Some(buffer_views.len()),
            byte_offset: color_offset,
            count,
            component_type: gltf::ComponentType::UNSIGNED_BYTE,
            type_: gltf::AccessorType::VEC4,
            normalized: true,
            sparse: None,
        });
    }
    if flags.joints {
        ret.attributes.joints_0 = Some(accessors.len());
        accessors.push(gltf::Accessor {
            buffer_view: Some(buffer_views.len()),
            byte_offset: joints_offset,
            count,
            component_type: gltf::ComponentType::UNSIGNED_SHORT,
            type_: gltf::AccessorType::VEC4,
            normalized: false,
            sparse: None,
        });
    }
    if flags.weights {
        ret.attributes.weights_0 = Some(accessors.len());
        accessors.push(gltf::Accessor {
            buffer_view: Some(buffer_views.len()),
            byte_offset: weights_offset,
            count,
            component_type: gltf::ComponentType::UNSIGNED_SHORT,
            type_: gltf::AccessorType::VEC4,
            normalized: true,
            sparse: None,
        });
    }
    if flags.index {
        ret.indices = Some(accessors.len());
        accessors.push(gltf::Accessor {
            buffer_view: Some(buffer_views.len() + 1),
            byte_offset: 0,
            count: indices.len(),
            component_type: gltf::ComponentType::UNSIGNED_INT,
            type_: gltf::AccessorType::SCALAR,
            normalized: false,
            sparse: None,
        });
    }

    buffer_views.push(view);
    if flags.index {
        buffer_views.push(gltf::BufferView {
            buffer: 0,
            byte_offset: buffer.len(),
            byte_length: indices.len() * 4,
            byte_stride: 0,
        });
        buffer.extend(indices.into_iter().flat_map(|v| (v as u32).to_le_bytes()));
    }

    // Blends
    let mut position = Vec::new();
    let mut normal = Vec::new();
    let mut tangent = Vec::new();
    let mut uv = Vec::new();
    for a in &mesh.blend {
        for i in a {
            #[allow(irrefutable_let_patterns)]
            if let parse::BlendData::ShiftVertex {
                index,
                position: p,
                normal: n,
                tangent: t,
                uv: uv_,
            } = i
            {
                let index = data_index[*index];
                position.extend(p.iter().map(|&(i, v)| (i + index, v)));
                normal.extend(n.iter().map(|&(i, v)| (i + index, v)));
                tangent.extend(t.iter().map(|&(i, v)| (i + index, v)));
                uv.extend(uv_.iter().map(|&(i, v)| (i + index, v)));
            }
        }

        fn orderize<T>(data: &mut Vec<(usize, T)>) {
            data.reverse();
            data.sort_by_key(|&(i, _)| i);
            data.dedup_by_key(|&mut (i, _)| i);
        }
        orderize(&mut position);
        orderize(&mut normal);
        orderize(&mut tangent);
        orderize(&mut uv);

        let mut attrs = gltf::MeshAttribute::default();

        if !position.is_empty() {
            attrs.position = Some(gltf.accessors.len());
            gltf.accessors.push(gltf::Accessor {
                buffer_view: None,
                byte_offset: 0,
                count,
                component_type: gltf::ComponentType::FLOAT,
                type_: gltf::AccessorType::VEC3,
                normalized: false,
                sparse: Some(gltf::AccessorSparse {
                    count: position.len(),
                    values: gltf::SparseValues {
                        buffer_view: gltf.buffer_views.len(),
                        byte_offset: 0,
                    },
                    indices: gltf::SparseIndices {
                        buffer_view: gltf.buffer_views.len() + 1,
                        byte_offset: 0,
                        component_type: gltf::ComponentType::UNSIGNED_SHORT,
                    },
                }),
            });

            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: position.len() * POSITION_SIZE,
                byte_stride: 0,
            });
            buffer.extend(position.iter().flat_map(|(_, v)| to_bytes(v)));
            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: position.len() * 2,
                byte_stride: 0,
            });
            add_short_indices(buffer, position.drain(..).map(|(v, _)| v as u16));
        }
        if !normal.is_empty() {
            attrs.normal = Some(gltf.accessors.len());
            gltf.accessors.push(gltf::Accessor {
                buffer_view: None,
                byte_offset: 0,
                count,
                component_type: gltf::ComponentType::FLOAT,
                type_: gltf::AccessorType::VEC3,
                normalized: false,
                sparse: Some(gltf::AccessorSparse {
                    count: normal.len(),
                    values: gltf::SparseValues {
                        buffer_view: gltf.buffer_views.len(),
                        byte_offset: 0,
                    },
                    indices: gltf::SparseIndices {
                        buffer_view: gltf.buffer_views.len() + 1,
                        byte_offset: 0,
                        component_type: gltf::ComponentType::UNSIGNED_SHORT,
                    },
                }),
            });

            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: normal.len() * NORMAL_SIZE,
                byte_stride: 0,
            });
            buffer.extend(normal.iter().flat_map(|(_, v)| to_bytes(v)));
            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: normal.len() * 2,
                byte_stride: 0,
            });
            add_short_indices(buffer, normal.drain(..).map(|(v, _)| v as u16));
        }
        if !tangent.is_empty() {
            attrs.tangent = Some(gltf.accessors.len());
            gltf.accessors.push(gltf::Accessor {
                buffer_view: None,
                byte_offset: 0,
                count,
                component_type: gltf::ComponentType::FLOAT,
                type_: gltf::AccessorType::VEC4,
                normalized: false,
                sparse: Some(gltf::AccessorSparse {
                    count: tangent.len(),
                    values: gltf::SparseValues {
                        buffer_view: gltf.buffer_views.len(),
                        byte_offset: 0,
                    },
                    indices: gltf::SparseIndices {
                        buffer_view: gltf.buffer_views.len() + 1,
                        byte_offset: 0,
                        component_type: gltf::ComponentType::UNSIGNED_SHORT,
                    },
                }),
            });

            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: tangent.len() * 4 * 3,
                byte_stride: 0,
            });
            buffer.extend(tangent.iter().flat_map(|(_, v)| to_bytes(v)));
            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: tangent.len() * 2,
                byte_stride: 0,
            });
            add_short_indices(buffer, tangent.drain(..).map(|(v, _)| v as u16));
        }
        if !uv.is_empty() {
            attrs.texcoord_0 = Some(gltf.accessors.len());
            gltf.accessors.push(gltf::Accessor {
                buffer_view: None,
                byte_offset: 0,
                count,
                component_type: gltf::ComponentType::FLOAT,
                type_: gltf::AccessorType::VEC2,
                normalized: false,
                sparse: Some(gltf::AccessorSparse {
                    count: uv.len(),
                    values: gltf::SparseValues {
                        buffer_view: gltf.buffer_views.len(),
                        byte_offset: 0,
                    },
                    indices: gltf::SparseIndices {
                        buffer_view: gltf.buffer_views.len() + 1,
                        byte_offset: 0,
                        component_type: gltf::ComponentType::UNSIGNED_SHORT,
                    },
                }),
            });

            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: uv.len() * UV_SIZE,
                byte_stride: 0,
            });
            buffer.extend(uv.iter().flat_map(|(_, v)| to_bytes(v)));
            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: uv.len() * 2,
                byte_stride: 0,
            });
            add_short_indices(buffer, uv.drain(..).map(|(v, _)| v as u16));
        }

        ret.targets.push(attrs);
    }

    Ok(ret)
}
