//! Generated material page contracts.
//!
//! "Textureless" means Ashfall should not depend mainly on unique hand-painted
//! texture skins. It does not mean flat colors. Beauty Mode needs generated and
//! cached pages for normal, height, roughness, dirt, wetness, cracks, oil, soot,
//! corrosion, and decals.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyMaterialIdV15(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyMaterialPageIdV15(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialPageKindV15 {
    BaseColor,
    Normal,
    Height,
    Roughness,
    Metallic,
    DirtMask,
    WetnessMask,
    CrackChipMask,
    SootOilCorrosionMask,
    DecalMask,
    SubsurfaceMask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialPageResolutionV15 {
    R128,
    R256,
    R512,
    R1024,
    R2048,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalSurfaceStateV15 {
    pub wetness: f32,
    pub dirt: f32,
    pub soot: f32,
    pub corrosion: f32,
    pub crack_density: f32,
    pub oil: f32,
    pub heat: f32,
    pub traffic_wear: f32,
}

impl Default for PhysicalSurfaceStateV15 {
    fn default() -> Self {
        Self {
            wetness: 0.0,
            dirt: 0.0,
            soot: 0.0,
            corrosion: 0.0,
            crack_density: 0.0,
            oil: 0.0,
            heat: 0.0,
            traffic_wear: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialPageRequestV15 {
    pub page_id: BeautyMaterialPageIdV15,
    pub material_id: BeautyMaterialIdV15,
    pub receiver_surface_id: u64,
    pub kind: MaterialPageKindV15,
    pub resolution: MaterialPageResolutionV15,
    pub world_bounds_min: [f32; 3],
    pub world_bounds_max: [f32; 3],
    pub state: PhysicalSurfaceStateV15,
    pub deterministic_seed: u64,
    pub priority: f32,
    pub can_rebuild: bool,
}

impl MaterialPageRequestV15 {
    pub fn wet_asphalt(
        receiver_surface_id: u64,
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        seed: u64,
    ) -> Vec<Self> {
        let base_state = PhysicalSurfaceStateV15 {
            wetness: 0.82,
            dirt: 0.65,
            soot: 0.18,
            corrosion: 0.0,
            crack_density: 0.32,
            oil: 0.2,
            heat: 0.0,
            traffic_wear: 0.75,
        };
        let material_id = BeautyMaterialIdV15(0xA5F_A17_u64);
        [
            MaterialPageKindV15::BaseColor,
            MaterialPageKindV15::Normal,
            MaterialPageKindV15::Height,
            MaterialPageKindV15::Roughness,
            MaterialPageKindV15::DirtMask,
            MaterialPageKindV15::WetnessMask,
            MaterialPageKindV15::CrackChipMask,
            MaterialPageKindV15::SootOilCorrosionMask,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| Self {
            page_id: BeautyMaterialPageIdV15(hash_page(receiver_surface_id, seed, index as u64)),
            material_id,
            receiver_surface_id,
            kind,
            resolution: MaterialPageResolutionV15::R1024,
            world_bounds_min: bounds_min,
            world_bounds_max: bounds_max,
            state: base_state,
            deterministic_seed: seed ^ (index as u64).wrapping_mul(0x9E37_79B9),
            priority: 0.9,
            can_rebuild: true,
        })
        .collect()
    }
}

fn hash_page(receiver_surface_id: u64, seed: u64, channel: u64) -> u128 {
    let a = receiver_surface_id as u128;
    let b = seed as u128;
    let c = channel as u128;
    (a << 64) ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ c
}
