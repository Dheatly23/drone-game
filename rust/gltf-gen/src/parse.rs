use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::marker::PhantomData;
use std::ops::BitOr;
use std::path::PathBuf;
use std::str::FromStr;

use nalgebra::{Matrix3x4, Matrix4, Translation3, UnitQuaternion, Vector2, Vector3, Vector4};
use serde::{de, Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct Data {
    pub meshes: HashMap<String, Mesh>,
    pub nodes: HashMap<String, Node>,
    pub materials: HashMap<String, Material>,
    pub skeletons: HashMap<String, Skeleton>,
    pub animations: HashMap<String, Animation>,

    pub root_node: String,
    #[serde(skip)]
    pub filepath: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct Node {
    #[serde(default)]
    pub translation: Translation3<f32>,
    #[serde(default)]
    pub rotation: UnitQuaternion<f32>,
    #[serde(default = "default_scale")]
    pub scale: Vector3<f32>,

    #[serde(default)]
    pub children: Vec<String>,

    #[serde(default)]
    pub mesh: Vec<String>,
    #[serde(default)]
    pub skin: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Mesh {
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub generate: AttrFlags,

    pub data: Vec<MeshData>,
    pub material: String,
    #[serde(default)]
    pub blend: Vec<Vec<BlendData>>,
}

#[derive(Debug, Deserialize, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttrFlags {
    #[serde(default)]
    pub normal: bool,
    #[serde(default)]
    pub tangent: bool,
    #[serde(default)]
    pub uv: bool,
    #[serde(default)]
    pub color: bool,
    #[serde(default)]
    pub joints: bool,
    #[serde(default)]
    pub weights: bool,
    #[serde(default)]
    pub index: bool,
}

impl BitOr for AttrFlags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self {
            normal: self.normal || other.normal,
            tangent: self.tangent || other.tangent,
            uv: self.uv || other.uv,
            color: self.color || other.color,
            joints: self.joints || other.joints,
            weights: self.weights || other.weights,
            index: self.index || other.index,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
pub enum Transform {
    #[default]
    Identity,
    Matrix {
        matrix: Matrix3x4<f32>,
    },
    Trs {
        #[serde(flatten)]
        data: TransformTRS,
    },
}

#[derive(Debug, Deserialize, Default)]
pub struct TransformTRS {
    #[serde(default)]
    pub translation: Translation3<f32>,
    #[serde(default)]
    pub rotation: UnitQuaternion<f32>,
    #[serde(default = "default_scale")]
    pub scale: Vector3<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MeshData {
    Triangles {
        position: Vec<Vector3<f32>>,
        #[serde(default)]
        normal: Vec<Vector3<f32>>,
        #[serde(default)]
        tangent: Vec<Vector4<f32>>,
        #[serde(default)]
        uv: Vec<Vector2<f32>>,
        #[serde(default)]
        color: Vec<[u8; 4]>,
        #[serde(default)]
        joints: Vec<[u16; 4]>,
        #[serde(default)]
        weights: Vec<Vector4<f32>>,

        #[serde(default)]
        index: Vec<usize>,
    },
    Plane {
        #[serde(flatten)]
        trs: TransformTRS,

        #[serde(flatten)]
        plane: PlaneData,
        #[serde(default, flatten)]
        joint: Option<JointData>,
    },
    PlaneJoint {
        #[serde(flatten)]
        trs: TransformTRS,

        #[serde(flatten)]
        plane: PlaneData,
        j1: [u16; 4],
        w1: Vector4<f32>,
        j2: [u16; 4],
        w2: Vector4<f32>,
        j3: [u16; 4],
        w3: Vector4<f32>,
        j4: [u16; 4],
        w4: Vector4<f32>,
    },
    GridPlaneSimple {
        #[serde(flatten)]
        trs: TransformTRS,

        #[serde(flatten)]
        plane: PlaneData,
        #[serde(default, flatten)]
        joint: Option<JointData>,

        size: [usize; 2],
        grid: Vec<u8>,
    },
    GridPlaneJointed {
        #[serde(flatten)]
        trs: TransformTRS,

        #[serde(flatten)]
        plane: PlaneData,
        joints: [u16; 4],
        w1: Vector4<f32>,
        w2: Vector4<f32>,
        w3: Vector4<f32>,
        w4: Vector4<f32>,
        #[serde(default)]
        mesh_type: GridPlaneMeshType,

        size: [usize; 2],
        grid: Vec<u8>,
    },
    VoxelSimple {
        #[serde(flatten)]
        trs: TransformTRS,

        p1: Vector3<f32>,
        p2: Vector3<f32>,
        p3: Vector3<f32>,
        p4: Vector3<f32>,
        p5: Vector3<f32>,
        p6: Vector3<f32>,
        p7: Vector3<f32>,
        p8: Vector3<f32>,
        #[serde(default, flatten)]
        joint: Option<JointData>,

        #[serde(default)]
        color: Option<[u8; 4]>,
        size: [usize; 3],
        grid: Vec<u8>,
    },
}

#[derive(Debug, Deserialize)]
pub struct PlaneData {
    pub p1: Vector3<f32>,
    pub p2: Vector3<f32>,
    pub p3: Vector3<f32>,
    pub p4: Vector3<f32>,
    #[serde(default)]
    pub flip: bool,

    #[serde(default)]
    pub normal: Option<Vector3<f32>>,
    #[serde(default)]
    pub tangent: Option<Vector4<f32>>,
    #[serde(default)]
    pub color: Option<[u8; 4]>,

    #[serde(default)]
    pub uv: Option<Vector2<f32>>,
    #[serde(default)]
    pub duv: Option<Vector2<f32>>,
    #[serde(default)]
    pub uv_swap: bool,
}

#[derive(Debug, Deserialize)]
pub struct JointData {
    pub joints: [u16; 4],
    pub weights: Vector4<f32>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GridPlaneMeshType {
    #[default]
    Quad,
    QuadCenter,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlendData {
    ShiftVertex {
        #[serde(default)]
        index: usize,

        position: Vec<(usize, Vector3<f32>)>,
        #[serde(default)]
        normal: Vec<(usize, Vector3<f32>)>,
        #[serde(default)]
        tangent: Vec<(usize, Vector3<f32>)>,
        #[serde(default)]
        uv: Vec<(usize, Vector2<f32>)>,
    },
}

#[derive(Debug, Deserialize)]
pub struct Material {
    #[serde(default = "default_color")]
    pub color: [u8; 4],
    #[serde(default)]
    pub metallic: Option<f32>,
    #[serde(default)]
    pub roughness: Option<f32>,
    #[serde(default)]
    pub normal_scale: Option<f32>,

    #[serde(default, deserialize_with = "string_or_struct")]
    pub color_texture: SampleTexture,
    #[serde(default, deserialize_with = "string_or_struct")]
    pub metallic_roughness_texture: SampleTexture,
    #[serde(default, deserialize_with = "string_or_struct")]
    pub normal_texture: SampleTexture,
    #[serde(default)]
    pub alpha_cutoff: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SampleTexture {
    pub filename: String,
    pub filter: SampleFilter,
    pub wrapping: SampleWrap,
}

impl FromStr for SampleTexture {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            filename: s.to_owned(),
            ..Self::default()
        })
    }
}

#[derive(Debug, Default, Deserialize)]
pub enum SampleFilter {
    #[default]
    Linear,
    Nearest,
}

#[derive(Debug, Default, Deserialize)]
pub enum SampleWrap {
    #[default]
    Repeat,
    MirrorRepeat,
    Clamp,
}

#[derive(Debug, Deserialize)]
pub struct Skeleton {
    pub root: String,
    #[serde(default)]
    pub joints: HashMap<String, Joint>,
}

#[derive(Debug, Deserialize)]
pub struct Joint {
    #[serde(default)]
    pub bind_matrix: Option<Matrix4<f32>>,
}

#[derive(Debug, Deserialize)]
pub struct Animation {
    #[serde(default)]
    pub interpolation: Interpolation,
    #[serde(default = "default_key_initial")]
    pub key_initial: bool,
    #[serde(default = "default_timescale")]
    pub timescale: f32,

    pub keyframe: Vec<AnimationKeyframe>,
}

#[inline]
fn default_key_initial() -> bool {
    true
}

#[inline]
fn default_timescale() -> f32 {
    1.0
}

impl Animation {
    pub fn orderize(&mut self) {
        self.keyframe.sort_by(|a, b| a.time.total_cmp(&b.time));
    }
}

#[derive(Debug, Deserialize)]
pub struct AnimationKeyframe {
    pub time: f32,
    #[serde(deserialize_with = "string_or_strings")]
    pub node: Vec<String>,

    #[serde(flatten)]
    pub data: AnimationKeyframeData,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnimationKeyframeData {
    Move {
        direction: Vector3<f32>,
    },
    Rotate {
        axis: Vector3<f32>,
        angle: f32,
    },
    Scale {
        factor: Vector3<f32>,
    },
    Keep {
        #[serde(default)]
        position: bool,
        #[serde(default)]
        rotation: bool,
        #[serde(default)]
        scale: bool,
    },
    Reset {
        #[serde(default)]
        position: bool,
        #[serde(default)]
        rotation: bool,
        #[serde(default)]
        scale: bool,
    },
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Step,
    #[default]
    Linear,
    Cubic,
}

#[inline]
const fn default_scale() -> Vector3<f32> {
    Vector3::new(1.0, 1.0, 1.0)
}

#[inline]
const fn default_color() -> [u8; 4] {
    [u8::MAX; 4]
}

fn string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + FromStr,
    T::Err: fmt::Display,
    D: Deserializer<'de>,
{
    // This is a Visitor that forwards string types to T's `FromStr` impl and
    // forwards map types to T's `Deserialize` impl. The `PhantomData` is to
    // keep the compiler from complaining about T being an unused generic type
    // parameter. We need T in order to know the Value type for the Visitor
    // impl.
    struct StringOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> de::Visitor<'de> for StringOrStruct<T>
    where
        T: Deserialize<'de> + FromStr,
        T::Err: fmt::Display,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E>
        where
            E: de::Error,
        {
            FromStr::from_str(value).map_err(|e| E::custom(e))
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            // `MapAccessDeserializer` is a wrapper that turns a `MapAccess`
            // into a `Deserializer`, allowing it to be used as the input to T's
            // `Deserialize` implementation. T then deserializes itself using
            // the entries from the map visitor.
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}

fn string_or_strings<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    T: Deserialize<'de> + FromStr,
    T::Err: fmt::Display,
    D: Deserializer<'de>,
{
    // This is a Visitor that forwards string types to T's `FromStr` impl and
    // forwards map types to T's `Deserialize` impl. The `PhantomData` is to
    // keep the compiler from complaining about T being an unused generic type
    // parameter. We need T in order to know the Value type for the Visitor
    // impl.
    struct StringOrStrings<T>(PhantomData<fn() -> T>);

    impl<'de, T> de::Visitor<'de> for StringOrStrings<T>
    where
        T: Deserialize<'de> + FromStr,
        T::Err: fmt::Display,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            match FromStr::from_str(value) {
                Ok(v) => Ok(vec![v]),
                Err(e) => Err(E::custom(e)),
            }
        }

        fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }
    }

    deserializer.deserialize_any(StringOrStrings(PhantomData))
}
