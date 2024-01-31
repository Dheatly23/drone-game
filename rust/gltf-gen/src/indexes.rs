// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::hash_map::{Entry, HashMap};

use super::gltf;

#[derive(Debug, Default)]
pub struct Index<'a> {
    pub named_node: HashMap<&'a str, Option<usize>>,
    pub named_mesh: HashMap<&'a str, gltf::MeshPrimitive>,
    pub named_material: HashMap<&'a str, usize>,
    pub named_skin: HashMap<&'a str, usize>,
    pub image_file: HashMap<&'a str, usize>,

    pub sampler: HashMap<gltf::Sampler, usize>,
    pub texture: HashMap<gltf::Texture, usize>,
}

impl<'a> Index<'a> {
    pub fn maybe_add_image<F, E>(&mut self, filename: &'a str, f: F) -> Result<usize, E>
    where
        F: FnOnce(&str) -> Result<usize, E>,
    {
        Ok(match self.image_file.entry(filename) {
            Entry::Occupied(v) => *v.get(),
            Entry::Vacant(v) => {
                let t = f(v.key())?;
                *v.insert(t)
            }
        })
    }

    pub fn cache_sampler(&mut self, sampler: gltf::Sampler, gltf: &mut gltf::Gltf) -> usize {
        *self.sampler.entry(sampler).or_insert_with_key(|k| {
            let ret = gltf.samplers.len();
            gltf.samplers.push(k.clone());
            ret
        })
    }

    pub fn cache_texture(&mut self, texture: gltf::Texture, gltf: &mut gltf::Gltf) -> usize {
        *self.texture.entry(texture).or_insert_with_key(|k| {
            let ret = gltf.textures.len();
            gltf.textures.push(k.clone());
            ret
        })
    }
}
