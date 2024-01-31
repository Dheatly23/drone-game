#![allow(dead_code)]

use nalgebra::{Quaternion, Vector3};
use serde::{Serialize, Serializer};

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Gltf {
    pub asset: Asset,
    pub buffers: Vec<Buffer>,
    pub buffer_views: Vec<BufferView>,
    pub accessors: Vec<Accessor>,
    pub images: Vec<Image>,
    pub textures: Vec<Texture>,
    pub samplers: Vec<Sampler>,
    pub materials: Vec<Material>,
    pub meshes: Vec<Mesh>,
    pub nodes: Vec<Node>,
    pub skins: Vec<Skin>,
    pub animations: Vec<Animation>,
    pub scenes: Vec<Scene>,
    pub scene: usize,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub version: String,
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub copyright: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Buffer {
    pub byte_length: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BufferView {
    pub buffer: usize,
    #[serde(skip_serializing_if = "skip_if_zero")]
    pub byte_offset: usize,
    pub byte_length: usize,
    #[serde(skip_serializing_if = "skip_if_zero")]
    pub byte_stride: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Accessor {
    #[serde(skip_serializing_if = "skip_if_none")]
    pub buffer_view: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_zero")]
    pub byte_offset: usize,
    pub component_type: ComponentType,
    #[serde(skip_serializing_if = "skip_if_false")]
    pub normalized: bool,
    pub count: usize,
    #[serde(rename = "type")]
    pub type_: AccessorType,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub sparse: Option<AccessorSparse>,
}

#[inline]
const fn skip_if_zero(&offset: &usize) -> bool {
    offset == 0
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessorSparse {
    pub count: usize,
    pub indices: SparseIndices,
    pub values: SparseValues,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SparseIndices {
    pub buffer_view: usize,
    #[serde(skip_serializing_if = "skip_if_zero")]
    pub byte_offset: usize,
    pub component_type: ComponentType,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SparseValues {
    pub buffer_view: usize,
    #[serde(skip_serializing_if = "skip_if_zero")]
    pub byte_offset: usize,
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentType {
    BYTE,
    UNSIGNED_BYTE,
    SHORT,
    UNSIGNED_SHORT,
    INT,
    UNSIGNED_INT,
    FLOAT,
}

impl Serialize for ComponentType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(match self {
            Self::BYTE => 5120,
            Self::UNSIGNED_BYTE => 5121,
            Self::SHORT => 5122,
            Self::UNSIGNED_SHORT => 5123,
            Self::INT => 5124,
            Self::UNSIGNED_INT => 5125,
            Self::FLOAT => 5126,
        })
    }
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AccessorType {
    #[default]
    SCALAR,
    VEC2,
    VEC3,
    VEC4,
    MAT2,
    MAT3,
    MAT4,
}

impl Serialize for AccessorType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::SCALAR => "SCALAR",
            Self::VEC2 => "VEC2",
            Self::VEC3 => "VEC3",
            Self::VEC4 => "VEC4",
            Self::MAT2 => "MAT2",
            Self::MAT3 => "MAT3",
            Self::MAT4 => "MAT4",
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub mime_type: ImageType,
    pub buffer_view: usize,
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageType {
    JPEG,
    PNG,
}

impl Serialize for ImageType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::JPEG => "image/jpeg",
            Self::PNG => "image/png",
        })
    }
}

#[derive(Debug, Default, Serialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Sampler {
    pub mag_filter: MagFilter,
    pub min_filter: MinFilter,
    pub wrap_s: WrapUV,
    pub wrap_t: WrapUV,
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MagFilter {
    NEAREST,
    #[default]
    LINEAR,
}

impl Serialize for MagFilter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(match self {
            Self::NEAREST => 9728,
            Self::LINEAR => 9729,
        })
    }
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MinFilter {
    NEAREST,
    #[default]
    LINEAR,
    NEAREST_MIPMAP_NEAREST,
    LINEAR_MIPMAP_NEAREST,
    NEAREST_MIPMAP_LINEAR,
    LINEAR_MIPMAP_LINEAR,
}

impl Serialize for MinFilter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(match self {
            Self::NEAREST => 9728,
            Self::LINEAR => 9729,
            Self::NEAREST_MIPMAP_NEAREST => 9984,
            Self::LINEAR_MIPMAP_NEAREST => 9985,
            Self::NEAREST_MIPMAP_LINEAR => 9986,
            Self::LINEAR_MIPMAP_LINEAR => 9987,
        })
    }
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WrapUV {
    CLAMP_TO_EDGE,
    MIRRORED_REPEAT,
    #[default]
    REPEAT,
}

impl Serialize for WrapUV {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(match self {
            Self::CLAMP_TO_EDGE => 33071,
            Self::MIRRORED_REPEAT => 33648,
            Self::REPEAT => 10497,
        })
    }
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Texture {
    pub sampler: usize,
    pub source: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mesh {
    pub primitives: Vec<MeshPrimitive>,
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub weights: Vec<f32>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MeshPrimitive {
    pub attributes: MeshAttribute,
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub targets: Vec<MeshAttribute>,

    #[serde(skip_serializing_if = "skip_if_none")]
    pub indices: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub material: Option<usize>,
}

#[derive(Debug, Serialize, Default, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct MeshAttribute {
    #[serde(skip_serializing_if = "skip_if_none")]
    pub position: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub normal: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub tangent: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub texcoord_0: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub color_0: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub joints_0: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub weights_0: Option<usize>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub children: Vec<usize>,

    pub translation: Vector3<f32>,
    pub rotation: Quaternion<f32>,
    pub scale: Vector3<f32>,

    #[serde(skip_serializing_if = "skip_if_none")]
    pub mesh: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub skin: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub name: String,

    #[serde(skip_serializing_if = "skip_if_none")]
    pub inverse_bind_matrices: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub skeleton: Option<usize>,
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub joints: Vec<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    #[serde(skip_serializing_if = "skip_if_none")]
    pub pbr_metallic_roughness: Option<PBRMetallicRoughness>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub normal_texture: Option<NormalTexture>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub occlusion_texture: Option<OcclusionTexture>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub emissive_texture: Option<TextureInfo>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub emissive_factor: Option<[f32; 3]>,

    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    #[serde(skip_serializing_if = "skip_if_false")]
    pub double_sided: bool,
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AlphaMode {
    #[default]
    OPAQUE,
    MASK,
    BLEND,
}

impl Serialize for AlphaMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::OPAQUE => "OPAQUE",
            Self::MASK => "MASK",
            Self::BLEND => "BLEND",
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PBRMetallicRoughness {
    #[serde(skip_serializing_if = "skip_if_none")]
    pub base_color_factor: Option<[f32; 4]>,
    #[serde(skip_serializing_if = "skip_if_none")]
    pub base_color_texture: Option<TextureInfo>,

    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub metallic_roughness_texture: Option<TextureInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalTexture {
    pub index: usize,
    pub scale: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcclusionTexture {
    pub index: usize,
    pub strength: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureInfo {
    pub index: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub name: String,

    pub channels: Vec<AnimationChannel>,
    pub samplers: Vec<AnimationSampler>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationChannel {
    pub sampler: usize,
    pub target: AnimationTarget,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationTarget {
    pub node: usize,
    pub path: TargetPath,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationSampler {
    pub input: usize,
    pub output: usize,
    pub interpolation: Interpolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetPath {
    Translation,
    Rotation,
    Scale,
    Weights,
}

impl Serialize for TargetPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Translation => "translation",
            Self::Rotation => "rotation",
            Self::Scale => "scale",
            Self::Weights => "weights",
        })
    }
}

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interpolation {
    #[default]
    LINEAR,
    STEP,
    CUBICSPLINE,
}

impl Serialize for Interpolation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::LINEAR => "LINEAR",
            Self::STEP => "STEP",
            Self::CUBICSPLINE => "CUBICSPLINE",
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    #[serde(skip_serializing_if = "skip_if_empty")]
    pub nodes: Vec<usize>,
}

#[inline]
const fn skip_if_false(&normal: &bool) -> bool {
    !normal
}

#[inline]
fn skip_if_none<T>(data: &Option<T>) -> bool {
    data.is_none()
}

#[inline]
fn skip_if_empty<T, C: AsRef<[T]>>(data: &C) -> bool {
    data.as_ref().is_empty()
}
