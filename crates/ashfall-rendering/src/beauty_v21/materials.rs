//! Structured generated texture/material pages.
//!
//! These are not hand-painted unique skins. They are rebuildable generated material caches.

use super::geometry::{BeautyMaterialIdV21, BeautySurfaceIdV21};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexturePageResolutionV21 {
    Far128,
    Mid256,
    Near512,
    Hero1024,
    Hero2048,
}

impl TexturePageResolutionV21 {
    pub fn texels(self) -> u32 {
        match self {
            Self::Far128 => 128,
            Self::Mid256 => 256,
            Self::Near512 => 512,
            Self::Hero1024 => 1024,
            Self::Hero2048 => 2048,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureChannelV21 {
    BaseColor,
    Normal,
    Height,
    Roughness,
    AmbientOcclusion,
    Dirt,
    Wetness,
    CrackChip,
    SootOilCorrosion,
    OrganicMask,
    DecalMask,
    ClearCoat,
    Subsurface,
    HairAnisotropy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialClassV21 {
    WetAsphalt,
    DirtyConcrete,
    SoilMud,
    StoneRock,
    PlantLeaf,
    BarkWood,
    RustedMetal,
    LandfillPlastic,
    LandfillFabric,
    CardboardPaper,
    Glass,
    CarPaint,
    Rubber,
    HumanSkin,
    ClothingFabric,
    Hair,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceTextureRecipeV21 {
    pub material_id: BeautyMaterialIdV21,
    pub surface_id: BeautySurfaceIdV21,
    pub class: MaterialClassV21,
    pub resolution: TexturePageResolutionV21,
    pub meters_per_tile: f32,
    pub channels: Vec<TextureChannelV21>,
    pub normal_strength_0_to_1: f32,
    pub height_strength_0_to_1: f32,
    pub roughness_variation_0_to_1: f32,
    pub albedo_variation_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub wetness_response_0_to_1: f32,
    pub crack_chip_0_to_1: f32,
    pub organic_irregularity_0_to_1: f32,
    pub procedural_warp_0_to_1: f32,
    pub smear_risk_0_to_1: f32,
    pub page_cache_priority_0_to_1: f32,
}

impl SurfaceTextureRecipeV21 {
    pub fn new(
        surface_id: BeautySurfaceIdV21,
        material_id: BeautyMaterialIdV21,
        class: MaterialClassV21,
        meters_per_tile: f32,
    ) -> Self {
        Self {
            material_id,
            surface_id,
            class,
            resolution: TexturePageResolutionV21::Near512,
            meters_per_tile,
            channels: Vec::new(),
            normal_strength_0_to_1: 0.5,
            height_strength_0_to_1: 0.25,
            roughness_variation_0_to_1: 0.5,
            albedo_variation_0_to_1: 0.35,
            dirt_0_to_1: 0.35,
            wetness_response_0_to_1: 0.30,
            crack_chip_0_to_1: 0.10,
            organic_irregularity_0_to_1: 0.10,
            procedural_warp_0_to_1: 0.025,
            smear_risk_0_to_1: 0.025,
            page_cache_priority_0_to_1: 0.50,
        }
    }

    pub fn with_channels(mut self, channels: &[TextureChannelV21]) -> Self {
        self.channels = channels.to_vec();
        self
    }

    pub fn with_resolution(mut self, resolution: TexturePageResolutionV21) -> Self {
        self.resolution = resolution;
        self
    }

    pub fn with_surface_detail(
        mut self,
        normal: f32,
        height: f32,
        roughness: f32,
        albedo: f32,
    ) -> Self {
        self.normal_strength_0_to_1 = normal.clamp(0.0, 1.0);
        self.height_strength_0_to_1 = height.clamp(0.0, 1.0);
        self.roughness_variation_0_to_1 = roughness.clamp(0.0, 1.0);
        self.albedo_variation_0_to_1 = albedo.clamp(0.0, 1.0);
        self
    }

    pub fn with_weathering(mut self, dirt: f32, wetness: f32, cracks: f32, organic: f32) -> Self {
        self.dirt_0_to_1 = dirt.clamp(0.0, 1.0);
        self.wetness_response_0_to_1 = wetness.clamp(0.0, 1.0);
        self.crack_chip_0_to_1 = cracks.clamp(0.0, 1.0);
        self.organic_irregularity_0_to_1 = organic.clamp(0.0, 1.0);
        self
    }

    pub fn with_artifact_limits(mut self, procedural_warp: f32, smear_risk: f32) -> Self {
        self.procedural_warp_0_to_1 = procedural_warp.clamp(0.0, 1.0);
        self.smear_risk_0_to_1 = smear_risk.clamp(0.0, 1.0);
        self
    }

    pub fn with_cache_priority(mut self, priority: f32) -> Self {
        self.page_cache_priority_0_to_1 = priority.clamp(0.0, 1.0);
        self
    }

    pub fn has_channel(&self, channel: TextureChannelV21) -> bool {
        self.channels.contains(&channel)
    }

    pub fn is_structured_enough(&self) -> bool {
        self.meters_per_tile > 0.05
            && self.has_channel(TextureChannelV21::BaseColor)
            && self.has_channel(TextureChannelV21::Normal)
            && self.has_channel(TextureChannelV21::Roughness)
            && self.procedural_warp_0_to_1 <= 0.065
            && self.smear_risk_0_to_1 <= 0.075
            && self.normal_strength_0_to_1 > 0.08
            && self.roughness_variation_0_to_1 > 0.08
    }

    pub fn wet_asphalt(surface_id: BeautySurfaceIdV21, material_id: BeautyMaterialIdV21) -> Self {
        Self::new(surface_id, material_id, MaterialClassV21::WetAsphalt, 1.8)
            .with_surface_detail(0.86, 0.46, 0.78, 0.40)
            .with_weathering(0.70, 0.92, 0.34, 0.02)
            .with_artifact_limits(0.022, 0.025)
            .with_cache_priority(0.96)
            .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn dirty_concrete(
        surface_id: BeautySurfaceIdV21,
        material_id: BeautyMaterialIdV21,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV21::DirtyConcrete,
            1.15,
        )
        .with_surface_detail(0.80, 0.42, 0.74, 0.38)
        .with_weathering(0.78, 0.34, 0.40, 0.04)
        .with_artifact_limits(0.020, 0.025)
        .with_cache_priority(0.84)
        .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn soil_mud(surface_id: BeautySurfaceIdV21, material_id: BeautyMaterialIdV21) -> Self {
        Self::new(surface_id, material_id, MaterialClassV21::SoilMud, 0.80)
            .with_surface_detail(0.94, 0.84, 0.88, 0.58)
            .with_weathering(0.80, 0.70, 0.18, 0.92)
            .with_artifact_limits(0.030, 0.040)
            .with_cache_priority(0.82)
            .with_channels(&STANDARD_ORGANIC_SURFACE_CHANNELS)
    }

    pub fn stone(surface_id: BeautySurfaceIdV21, material_id: BeautyMaterialIdV21) -> Self {
        Self::new(surface_id, material_id, MaterialClassV21::StoneRock, 0.55)
            .with_surface_detail(0.88, 0.68, 0.72, 0.46)
            .with_weathering(0.62, 0.24, 0.50, 0.16)
            .with_artifact_limits(0.018, 0.024)
            .with_cache_priority(0.68)
            .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn plant_leaf(surface_id: BeautySurfaceIdV21, material_id: BeautyMaterialIdV21) -> Self {
        Self::new(surface_id, material_id, MaterialClassV21::PlantLeaf, 0.22)
            .with_surface_detail(0.64, 0.22, 0.56, 0.68)
            .with_weathering(0.24, 0.46, 0.04, 0.96)
            .with_artifact_limits(0.018, 0.022)
            .with_cache_priority(0.64)
            .with_channels(&[
                TextureChannelV21::BaseColor,
                TextureChannelV21::Normal,
                TextureChannelV21::Roughness,
                TextureChannelV21::AmbientOcclusion,
                TextureChannelV21::Wetness,
                TextureChannelV21::OrganicMask,
                TextureChannelV21::Subsurface,
            ])
    }

    pub fn landfill_plastic(
        surface_id: BeautySurfaceIdV21,
        material_id: BeautyMaterialIdV21,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV21::LandfillPlastic,
            0.72,
        )
        .with_surface_detail(0.62, 0.38, 0.82, 0.70)
        .with_weathering(0.90, 0.50, 0.16, 0.24)
        .with_artifact_limits(0.020, 0.028)
        .with_cache_priority(0.72)
        .with_channels(&[
            TextureChannelV21::BaseColor,
            TextureChannelV21::Normal,
            TextureChannelV21::Height,
            TextureChannelV21::Roughness,
            TextureChannelV21::Dirt,
            TextureChannelV21::Wetness,
            TextureChannelV21::DecalMask,
        ])
    }

    pub fn human_skin(surface_id: BeautySurfaceIdV21, material_id: BeautyMaterialIdV21) -> Self {
        Self::new(surface_id, material_id, MaterialClassV21::HumanSkin, 0.18)
            .with_resolution(TexturePageResolutionV21::Hero1024)
            .with_surface_detail(0.58, 0.12, 0.50, 0.30)
            .with_weathering(0.08, 0.16, 0.02, 0.02)
            .with_artifact_limits(0.010, 0.018)
            .with_cache_priority(0.92)
            .with_channels(&[
                TextureChannelV21::BaseColor,
                TextureChannelV21::Normal,
                TextureChannelV21::Roughness,
                TextureChannelV21::AmbientOcclusion,
                TextureChannelV21::Wetness,
                TextureChannelV21::Subsurface,
            ])
    }

    pub fn clothing_fabric(
        surface_id: BeautySurfaceIdV21,
        material_id: BeautyMaterialIdV21,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV21::ClothingFabric,
            0.35,
        )
        .with_surface_detail(0.66, 0.24, 0.76, 0.42)
        .with_weathering(0.38, 0.22, 0.02, 0.05)
        .with_artifact_limits(0.014, 0.024)
        .with_cache_priority(0.76)
        .with_channels(&[
            TextureChannelV21::BaseColor,
            TextureChannelV21::Normal,
            TextureChannelV21::Height,
            TextureChannelV21::Roughness,
            TextureChannelV21::AmbientOcclusion,
            TextureChannelV21::Dirt,
            TextureChannelV21::Wetness,
        ])
    }

    pub fn car_paint(surface_id: BeautySurfaceIdV21, material_id: BeautyMaterialIdV21) -> Self {
        Self::new(surface_id, material_id, MaterialClassV21::CarPaint, 1.25)
            .with_surface_detail(0.28, 0.08, 0.34, 0.18)
            .with_weathering(0.34, 0.56, 0.03, 0.01)
            .with_artifact_limits(0.010, 0.018)
            .with_cache_priority(0.86)
            .with_channels(&[
                TextureChannelV21::BaseColor,
                TextureChannelV21::Normal,
                TextureChannelV21::Roughness,
                TextureChannelV21::Dirt,
                TextureChannelV21::Wetness,
                TextureChannelV21::ClearCoat,
                TextureChannelV21::DecalMask,
            ])
    }
}

pub const STANDARD_HARD_SURFACE_CHANNELS: [TextureChannelV21; 9] = [
    TextureChannelV21::BaseColor,
    TextureChannelV21::Normal,
    TextureChannelV21::Height,
    TextureChannelV21::Roughness,
    TextureChannelV21::AmbientOcclusion,
    TextureChannelV21::Dirt,
    TextureChannelV21::Wetness,
    TextureChannelV21::CrackChip,
    TextureChannelV21::SootOilCorrosion,
];

pub const STANDARD_ORGANIC_SURFACE_CHANNELS: [TextureChannelV21; 8] = [
    TextureChannelV21::BaseColor,
    TextureChannelV21::Normal,
    TextureChannelV21::Height,
    TextureChannelV21::Roughness,
    TextureChannelV21::AmbientOcclusion,
    TextureChannelV21::Dirt,
    TextureChannelV21::Wetness,
    TextureChannelV21::OrganicMask,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProceduralMaterialSampleV21 {
    pub base_color_linear: [f32; 3],
    pub normal_xy: [f32; 2],
    pub height_0_to_1: f32,
    pub roughness_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub wetness_0_to_1: f32,
    pub structured_detail_score_0_to_1: f32,
}

/// Deterministic low-cost material sampler for early CPU/debug previews.
/// Production can replace this with Vulkan compute using the same recipe contract.
pub fn sample_material_v21(
    recipe: &SurfaceTextureRecipeV21,
    uv: [f32; 2],
    seed: u64,
) -> ProceduralMaterialSampleV21 {
    let n1 = hash01(seed ^ 0xA51F_F00D, uv[0], uv[1]);
    let n2 = hash01(seed ^ 0xC0DE_5021, uv[0] * 1.7, uv[1] * 0.9);
    let n3 = hash01(seed ^ 0xD17A_2021, uv[0] * 4.0, uv[1] * 4.0);
    let detail = (0.30 * n1 + 0.30 * n2 + 0.40 * n3).clamp(0.0, 1.0);
    let class_tint = match recipe.class {
        MaterialClassV21::WetAsphalt => [0.055, 0.058, 0.060],
        MaterialClassV21::DirtyConcrete => [0.42, 0.39, 0.34],
        MaterialClassV21::SoilMud => [0.19, 0.12, 0.07],
        MaterialClassV21::StoneRock => [0.33, 0.32, 0.30],
        MaterialClassV21::PlantLeaf => [0.07, 0.23, 0.07],
        MaterialClassV21::LandfillPlastic => [0.30, 0.28, 0.25],
        MaterialClassV21::HumanSkin => [0.65, 0.43, 0.32],
        MaterialClassV21::ClothingFabric => [0.16, 0.17, 0.21],
        MaterialClassV21::CarPaint => [0.16, 0.18, 0.20],
        _ => [0.25, 0.24, 0.22],
    };
    let albedo_var = recipe.albedo_variation_0_to_1 * (detail - 0.5) * 0.35;
    ProceduralMaterialSampleV21 {
        base_color_linear: [
            (class_tint[0] + albedo_var).clamp(0.0, 1.0),
            (class_tint[1] + albedo_var).clamp(0.0, 1.0),
            (class_tint[2] + albedo_var).clamp(0.0, 1.0),
        ],
        normal_xy: [
            (n1 - 0.5) * recipe.normal_strength_0_to_1,
            (n2 - 0.5) * recipe.normal_strength_0_to_1,
        ],
        height_0_to_1: (0.5 + (n3 - 0.5) * recipe.height_strength_0_to_1).clamp(0.0, 1.0),
        roughness_0_to_1: (0.58 + (n2 - 0.5) * recipe.roughness_variation_0_to_1).clamp(0.04, 0.98),
        dirt_0_to_1: (recipe.dirt_0_to_1 * (0.55 + 0.45 * n1)).clamp(0.0, 1.0),
        wetness_0_to_1: (recipe.wetness_response_0_to_1 * (0.45 + 0.55 * n2)).clamp(0.0, 1.0),
        structured_detail_score_0_to_1: (0.25 + detail * 0.75).clamp(0.0, 1.0),
    }
}

fn hash01(seed: u64, x: f32, y: f32) -> f32 {
    let mut h = seed ^ (x.to_bits() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (y.to_bits() as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h as f64 / u64::MAX as f64) as f32
}
