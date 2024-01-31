// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::hash_map::{Entry, HashMap};
use std::io::Cursor;
use std::path::Path;

use anyhow::{bail, Error};
use camino::{Utf8Path, Utf8PathBuf};
use image::io::Reader as ImageReader;
use image::ImageFormat;
use nalgebra::{Isometry3, Matrix4, Scale, Unit, UnitQuaternion, Vector3};

use super::indexes::Index;
use super::meshgen::generate_mesh;
use super::{gltf, parse};

pub fn add_image(path: &Path, gltf: &mut gltf::Gltf, buffer: &mut Vec<u8>) -> Result<(), Error> {
    let start = buffer.len();
    let end;

    {
        let img = ImageReader::open(path)?.decode()?;
        let mut cursor = Cursor::new(&mut *buffer);
        cursor.set_position(start as _);
        img.write_to(&mut cursor, ImageFormat::Png)?;
        end = cursor.position() as usize;
    }

    buffer.truncate(end);
    gltf.images.push(gltf::Image {
        mime_type: gltf::ImageType::PNG,
        buffer_view: gltf.buffer_views.len(),
    });
    gltf.buffer_views.push(gltf::BufferView {
        buffer: 0,
        byte_offset: start,
        byte_length: end - start,
        byte_stride: 0,
    });

    Ok(())
}

fn add_texture<'a>(
    texture: &'a parse::SampleTexture,
    parent: &mut Utf8PathBuf,
    gltf: &mut gltf::Gltf,
    buffer: &mut Vec<u8>,
    index: &mut Index<'a>,
) -> Result<usize, Error> {
    let source = index.maybe_add_image(&texture.filename, |file| {
        let path_ = Utf8Path::new(file);
        if path_.has_root() {
            bail!("Image file {path_} is an absolute path!");
        } else if path_.parent().is_some() {
            bail!("Image file {path_} is underneath directory");
        }

        parent.push(path_);
        let ret = gltf.images.len();
        add_image(parent.as_std_path(), gltf, buffer)?;
        parent.pop();
        Ok(ret)
    })?;

    let (mag_filter, min_filter) = match texture.filter {
        parse::SampleFilter::Linear => (gltf::MagFilter::LINEAR, gltf::MinFilter::LINEAR),
        parse::SampleFilter::Nearest => (gltf::MagFilter::NEAREST, gltf::MinFilter::NEAREST),
    };
    let wrap = match texture.wrapping {
        parse::SampleWrap::Repeat => gltf::WrapUV::REPEAT,
        parse::SampleWrap::MirrorRepeat => gltf::WrapUV::MIRRORED_REPEAT,
        parse::SampleWrap::Clamp => gltf::WrapUV::CLAMP_TO_EDGE,
    };
    let sampler = index.cache_sampler(
        gltf::Sampler {
            mag_filter,
            min_filter,
            wrap_s: wrap,
            wrap_t: wrap,
        },
        gltf,
    );

    Ok(index.cache_texture(gltf::Texture { sampler, source }, gltf))
}

fn add_material<'a>(
    parent: &Path,
    material: &'a parse::Material,
    gltf: &mut gltf::Gltf,
    buffer: &mut Vec<u8>,
    index: &mut Index<'a>,
) -> Result<(), Error> {
    let mut path = Utf8PathBuf::try_from(parent.to_path_buf())?;
    let mut mat = gltf::Material {
        pbr_metallic_roughness: None,
        normal_texture: None,
        occlusion_texture: None,
        emissive_factor: None,
        emissive_texture: None,
        alpha_mode: gltf::AlphaMode::OPAQUE,
        alpha_cutoff: 0.95,
        double_sided: false,
    };

    if material.metallic.is_some()
        || material.roughness.is_some()
        || !material.metallic_roughness_texture.filename.is_empty()
        || material.color != [u8::MAX; 4]
    {
        let mut v = gltf::PBRMetallicRoughness {
            base_color_factor: None,
            metallic_factor: material.metallic.unwrap_or(1.0),
            roughness_factor: material.roughness.unwrap_or(1.0),
            metallic_roughness_texture: None,
            base_color_texture: None,
        };

        let [r, g, b, a] = material.color;
        v.base_color_factor = Some([
            (r as f32) / 255.,
            (g as f32) / 255.,
            (b as f32) / 255.,
            (a as f32) / 255.,
        ]);

        if !material.color_texture.filename.is_empty() {
            v.base_color_texture = Some(gltf::TextureInfo {
                index: add_texture(&material.color_texture, &mut path, gltf, buffer, index)?,
            });
        }

        if !material.metallic_roughness_texture.filename.is_empty() {
            v.metallic_roughness_texture = Some(gltf::TextureInfo {
                index: add_texture(
                    &material.metallic_roughness_texture,
                    &mut path,
                    gltf,
                    buffer,
                    index,
                )?,
            });
        }

        mat.pbr_metallic_roughness = Some(v);
    }

    if !material.normal_texture.filename.is_empty() || material.normal_scale.is_some() {
        mat.normal_texture = Some(gltf::NormalTexture {
            index: add_texture(&material.normal_texture, &mut path, gltf, buffer, index)?,
            scale: material.normal_scale.unwrap_or(1.0),
        });
    }

    gltf.materials.push(mat);

    Ok(())
}

pub fn add_node<'a>(
    data: &'a parse::Data,
    name: &'a str,
    gltf: &mut gltf::Gltf,
    buffer: &mut Vec<u8>,
    index: &mut Index<'a>,
) -> Result<(), Error> {
    let Some(node) = data.nodes.get(name) else {
        bail!("No node named {name}")
    };
    let mut ret = gltf::Node {
        name: name.to_owned(),
        translation: node.translation.vector,
        rotation: node.rotation.into_inner(),
        scale: node.scale,
        ..gltf::Node::default()
    };

    let f = |mesh_name: &'a String| -> Result<_, Error> {
        if let Some(v) = index.named_mesh.get(&**mesh_name) {
            return Ok(v.clone());
        }

        let Some(mesh) = data.meshes.get(mesh_name) else {
            bail!("Error at node {name}: no mesh named {mesh_name}")
        };
        let mut prim = generate_mesh(mesh_name, mesh, gltf, buffer)?;
        if !mesh.material.is_empty() {
            prim.material = Some(match index.named_material.entry(&mesh.material) {
                Entry::Occupied(v) => *v.get(),
                Entry::Vacant(v) => {
                    let ret = *v.insert(gltf.materials.len());
                    add_material(
                        &data.filepath,
                        &data.materials[&mesh.material],
                        gltf,
                        buffer,
                        index,
                    )?;
                    ret
                }
            });
        }
        index.named_mesh.insert(mesh_name, prim.clone());
        Ok(prim)
    };
    let primitives = node.mesh.iter().map(f).collect::<Result<Vec<_>, _>>()?;
    if !primitives.is_empty() {
        ret.mesh = Some(gltf.meshes.len());
        gltf.meshes.push(gltf::Mesh {
            weights: vec![0.0; primitives.iter().fold(0, |a, v| a.max(v.targets.len()))],
            primitives,
        });
    }

    let f = |child: &'a String| -> Result<_, Error> {
        match index.named_node.entry(&**child) {
            Entry::Occupied(v) => match v.get() {
                Some(_) => bail!("Node {child} has more than 1 parent"),
                None => bail!(" Node {child} is referencing itself recursively"),
            },
            Entry::Vacant(v) => v.insert(None),
        };

        add_node(data, child, gltf, buffer, index)?;
        let v = gltf.nodes.len() - 1;
        index.named_node.insert(child, Some(v));
        Ok(v)
    };
    ret.children = node.children.iter().map(f).collect::<Result<Vec<_>, _>>()?;

    gltf.nodes.push(ret);
    Ok(())
}

pub fn add_skeleton<'a>(
    skeleton: &parse::Skeleton,
    name: &'a str,
    gltf: &mut gltf::Gltf,
    buffer: &mut Vec<u8>,
    index: &mut Index<'a>,
) -> Result<(), Error> {
    let Some(&Some(ix)) = index.named_node.get(&*skeleton.root) else {
        bail!("Node {} does not exist!", skeleton.root)
    };

    fn f(
        skeleton: &parse::Skeleton,
        gltf: &gltf::Gltf,
        buffer: &mut Vec<u8>,
        joints: &mut Vec<usize>,
        ix: usize,
        mat: &Matrix4<f32>,
    ) -> Result<(), Error> {
        let node = &gltf.nodes[ix];

        let mut matrix;
        if let Some(v) = skeleton.joints.get(&node.name).and_then(|v| v.bind_matrix) {
            matrix = v;
            if !matrix.try_inverse_mut() {
                bail!("Bind matrix {matrix} is non-invertible");
            }
        } else {
            let mut temp =
                Isometry3::from_parts(node.translation.into(), Unit::new_unchecked(node.rotation));
            temp.inverse_mut();
            matrix = temp.to_matrix();
            // SAFETY: Scale should be invertible
            matrix.append_nonuniform_scaling_mut(unsafe {
                &Scale::from(node.scale).inverse_unchecked().vector
            });
        };
        matrix *= mat;
        buffer.extend(matrix.iter().flat_map(|v| v.to_le_bytes()));
        joints.push(ix);

        for &c in &node.children {
            f(skeleton, gltf, buffer, joints, c, &matrix)?;
        }

        Ok(())
    }

    let mut view = gltf::BufferView {
        buffer: 0,
        byte_offset: buffer.len(),
        byte_length: 0,
        byte_stride: 0,
    };
    let mut joints = Vec::new();
    f(
        skeleton,
        gltf,
        buffer,
        &mut joints,
        ix,
        &Matrix4::identity(),
    )?;

    let count = joints.len();
    index.named_skin.insert(name, gltf.skins.len());
    gltf.skins.push(gltf::Skin {
        name: name.to_owned(),
        inverse_bind_matrices: if count > 0 {
            Some(gltf.accessors.len())
        } else {
            None
        },
        skeleton: Some(ix),
        joints,
    });
    if count > 0 {
        gltf.accessors.push(gltf::Accessor {
            buffer_view: Some(gltf.buffer_views.len()),
            byte_offset: 0,
            component_type: gltf::ComponentType::FLOAT,
            normalized: false,
            count,
            type_: gltf::AccessorType::MAT4,
            sparse: None,
        });
        view.byte_length = count * (4 * 4 * 4);
        gltf.buffer_views.push(view);
    }

    Ok(())
}

pub fn bind_skins<'a>(
    data: &'a parse::Data,
    gltf: &mut gltf::Gltf,
    buffer: &mut Vec<u8>,
    index: &mut Index<'a>,
) -> Result<(), Error> {
    for i in 0..gltf.nodes.len() {
        let node = &mut gltf.nodes[i];
        let Some(v) = &data.nodes[&*node.name].skin else {
            continue;
        };

        if let Some(&v) = index.named_skin.get(&**v) {
            node.skin = Some(v);
        } else if let Some(skeleton) = data.skeletons.get(v) {
            node.skin = Some(gltf.skins.len());
            add_skeleton(skeleton, v, gltf, buffer, index)?;
        } else {
            bail!("Skeleton {v} does not exist!")
        }
    }

    Ok(())
}

fn insert_keyframe<T>(v: &mut Vec<(f32, T)>, time: f32, t: T) {
    if let Some((lt, lv)) = v.last_mut() {
        if *lt == time {
            *lv = t;
            return;
        }
    }
    v.push((time, t));
}

pub fn add_animation(
    anim: &parse::Animation,
    name: &str,
    gltf: &mut gltf::Gltf,
    buffer: &mut Vec<u8>,
    index: &Index<'_>,
) -> Result<(), Error> {
    // Check time is ascending
    anim.keyframe.iter().fold(0.0, |a, b| {
        let b = b.time;
        assert!(b >= 0.0, "Negative keyframe time");
        assert!(a <= b, "Keyframe is not ascending");
        b
    });

    #[derive(Debug)]
    struct Inner {
        index: usize,
        position: Vector3<f32>,
        position_keys: Vec<(f32, Vector3<f32>)>,
        rotation: UnitQuaternion<f32>,
        rotation_keys: Vec<(f32, UnitQuaternion<f32>)>,
        scale: Vector3<f32>,
        scale_keys: Vec<(f32, Vector3<f32>)>,
    }

    let mut data = HashMap::new();
    fn f<'a>(
        data: &mut HashMap<&'a str, Inner>,
        gltf: &gltf::Gltf,
        index: &Index<'_>,
        name: &'a Vec<String>,
        mut f: impl FnMut(&mut Inner),
    ) -> Result<(), Error> {
        for name in name {
            let inner = match data.entry(name) {
                Entry::Occupied(v) => v.into_mut(),
                Entry::Vacant(v) => {
                    let Some(&Some(ix)) = index.named_node.get(&**name) else {
                        bail!("Node {name} does not exist!")
                    };
                    let node = &gltf.nodes[ix];
                    let r = v.insert(Inner {
                        index: ix,
                        position: node.translation,
                        position_keys: Vec::new(),
                        rotation: Unit::new_unchecked(node.rotation),
                        rotation_keys: Vec::new(),
                        scale: node.scale,
                        scale_keys: Vec::new(),
                    });
                    r
                }
            };
            f(inner)
        }
        Ok(())
    }

    for i in &anim.keyframe {
        let (time, node) = (i.time, &i.node);
        match &i.data {
            parse::AnimationKeyframeData::Move { direction } => {
                f(&mut data, gltf, index, node, |v| {
                    v.position += direction;
                    insert_keyframe(&mut v.position_keys, time, v.position);
                })
            }
            parse::AnimationKeyframeData::Rotate { axis, angle } => {
                f(&mut data, gltf, index, node, |v| {
                    v.rotation = UnitQuaternion::from_axis_angle(
                        &Unit::new_normalize(*axis),
                        angle.to_radians(),
                    ) * v.rotation;
                    v.rotation.renormalize_fast();
                    insert_keyframe(&mut v.rotation_keys, time, v.rotation);
                })
            }
            parse::AnimationKeyframeData::Scale { factor } => {
                f(&mut data, gltf, index, node, |v| {
                    v.scale.component_mul_assign(factor);
                    insert_keyframe(&mut v.scale_keys, time, v.scale);
                })
            }
            parse::AnimationKeyframeData::Keep {
                position,
                rotation,
                scale,
            } if !(*position || *rotation || *scale) => continue,
            parse::AnimationKeyframeData::Keep {
                position,
                rotation,
                scale,
            } => f(&mut data, gltf, index, node, |v| {
                if *position {
                    insert_keyframe(&mut v.position_keys, time, v.position);
                }
                if *rotation {
                    insert_keyframe(&mut v.rotation_keys, time, v.rotation);
                }
                if *scale {
                    insert_keyframe(&mut v.scale_keys, time, v.scale);
                }
            }),
            parse::AnimationKeyframeData::Reset {
                position,
                rotation,
                scale,
            } if !(*position || *rotation || *scale) => continue,
            parse::AnimationKeyframeData::Reset {
                position,
                rotation,
                scale,
            } => f(&mut data, gltf, index, node, |v| {
                let node = &gltf.nodes[v.index];
                if *position {
                    v.position = node.translation;
                    insert_keyframe(&mut v.position_keys, time, v.position);
                }
                if *rotation {
                    v.rotation = Unit::new_unchecked(node.rotation);
                    insert_keyframe(&mut v.rotation_keys, time, v.rotation);
                }
                if *scale {
                    v.scale = node.scale;
                    insert_keyframe(&mut v.scale_keys, time, v.scale);
                }
            }),
        }?
    }

    for v in data.values_mut() {
        for (t, _) in &mut v.position_keys {
            *t *= anim.timescale;
        }
        for (t, _) in &mut v.rotation_keys {
            *t *= anim.timescale;
        }
        for (t, _) in &mut v.scale_keys {
            *t *= anim.timescale;
        }

        if anim.key_initial {
            let node = &gltf.nodes[v.index];
            if v.position_keys.first().map(|&(t, _)| t) != Some(0.0) {
                v.position_keys.insert(0, (0.0, node.translation));
            }
            if v.rotation_keys.first().map(|&(t, _)| t) != Some(0.0) {
                v.rotation_keys
                    .insert(0, (0.0, Unit::new_unchecked(node.rotation)));
            }
            if v.scale_keys.first().map(|&(t, _)| t) != Some(0.0) {
                v.scale_keys.insert(0, (0.0, node.scale));
            }
        }
    }

    let interpolation = match anim.interpolation {
        parse::Interpolation::Step => gltf::Interpolation::STEP,
        parse::Interpolation::Linear => gltf::Interpolation::LINEAR,
        parse::Interpolation::Cubic => gltf::Interpolation::CUBICSPLINE,
    };
    let n = data
        .values()
        .map(|v| {
            (!v.position_keys.is_empty()) as usize
                + (!v.rotation_keys.is_empty()) as usize
                + (!v.scale_keys.is_empty()) as usize
        })
        .sum();
    let mut channels = Vec::with_capacity(n);
    let mut samplers = Vec::with_capacity(n);
    for (_, v) in data {
        let Inner {
            index: node,
            position_keys,
            rotation_keys,
            scale_keys,
            ..
        } = v;
        if position_keys.is_empty() && rotation_keys.is_empty() && scale_keys.is_empty() {
            continue;
        }

        if !position_keys.is_empty() {
            channels.push(gltf::AnimationChannel {
                sampler: samplers.len(),
                target: gltf::AnimationTarget {
                    node,
                    path: gltf::TargetPath::Translation,
                },
            });
            samplers.push(gltf::AnimationSampler {
                input: gltf.accessors.len(),
                output: gltf.accessors.len() + 1,
                interpolation,
            });

            gltf.accessors.extend([
                gltf::Accessor {
                    buffer_view: Some(gltf.buffer_views.len()),
                    byte_offset: 0,
                    component_type: gltf::ComponentType::FLOAT,
                    normalized: false,
                    count: position_keys.len(),
                    type_: gltf::AccessorType::SCALAR,
                    sparse: None,
                },
                gltf::Accessor {
                    buffer_view: Some(gltf.buffer_views.len()),
                    byte_offset: 4,
                    component_type: gltf::ComponentType::FLOAT,
                    normalized: false,
                    count: position_keys.len(),
                    type_: gltf::AccessorType::VEC3,
                    sparse: None,
                },
            ]);
            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: position_keys.len() * 4 * 4,
                byte_stride: 4 * 4,
            });
            buffer.extend(
                position_keys
                    .into_iter()
                    .flat_map(|(t, v)| [t, v.x, v.y, v.z])
                    .flat_map(|v| v.to_le_bytes()),
            );
        }
        if !rotation_keys.is_empty() {
            channels.push(gltf::AnimationChannel {
                sampler: samplers.len(),
                target: gltf::AnimationTarget {
                    node,
                    path: gltf::TargetPath::Rotation,
                },
            });
            samplers.push(gltf::AnimationSampler {
                input: gltf.accessors.len(),
                output: gltf.accessors.len() + 1,
                interpolation,
            });

            gltf.accessors.extend([
                gltf::Accessor {
                    buffer_view: Some(gltf.buffer_views.len()),
                    byte_offset: 0,
                    component_type: gltf::ComponentType::FLOAT,
                    normalized: false,
                    count: rotation_keys.len(),
                    type_: gltf::AccessorType::SCALAR,
                    sparse: None,
                },
                gltf::Accessor {
                    buffer_view: Some(gltf.buffer_views.len()),
                    byte_offset: 4,
                    component_type: gltf::ComponentType::FLOAT,
                    normalized: false,
                    count: rotation_keys.len(),
                    type_: gltf::AccessorType::VEC4,
                    sparse: None,
                },
            ]);
            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: rotation_keys.len() * 4 * 5,
                byte_stride: 4 * 5,
            });
            buffer.extend(
                rotation_keys
                    .into_iter()
                    .flat_map(|(t, v)| [t, v.i, v.j, v.k, v.w])
                    .flat_map(|v| v.to_le_bytes()),
            );
        }
        if !scale_keys.is_empty() {
            channels.push(gltf::AnimationChannel {
                sampler: samplers.len(),
                target: gltf::AnimationTarget {
                    node,
                    path: gltf::TargetPath::Scale,
                },
            });
            samplers.push(gltf::AnimationSampler {
                input: gltf.accessors.len(),
                output: gltf.accessors.len() + 1,
                interpolation,
            });

            gltf.accessors.extend([
                gltf::Accessor {
                    buffer_view: Some(gltf.buffer_views.len()),
                    byte_offset: 0,
                    component_type: gltf::ComponentType::FLOAT,
                    normalized: false,
                    count: scale_keys.len(),
                    type_: gltf::AccessorType::SCALAR,
                    sparse: None,
                },
                gltf::Accessor {
                    buffer_view: Some(gltf.buffer_views.len()),
                    byte_offset: 4,
                    component_type: gltf::ComponentType::FLOAT,
                    normalized: false,
                    count: scale_keys.len(),
                    type_: gltf::AccessorType::VEC3,
                    sparse: None,
                },
            ]);
            gltf.buffer_views.push(gltf::BufferView {
                buffer: 0,
                byte_offset: buffer.len(),
                byte_length: scale_keys.len() * 4 * 4,
                byte_stride: 4 * 4,
            });
            buffer.extend(
                scale_keys
                    .into_iter()
                    .flat_map(|(t, v)| [t, v.x, v.y, v.z])
                    .flat_map(|v| v.to_le_bytes()),
            );
        }
    }

    gltf.animations.push(gltf::Animation {
        name: name.to_owned(),
        channels,
        samplers,
    });

    Ok(())
}
