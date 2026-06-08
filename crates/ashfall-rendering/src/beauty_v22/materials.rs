//! Structured generated texture/material pages.
//!
//! Textureless means "not hand-painted unique skins".
//! It does not mean flat colors, raw noise, or procedural smears.

use super::geometry::{BeautyMaterialIdV22, BeautySurfaceIdV22};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexturePageResolutionV22 {
    Far128,
    Mid256,
    Near512,
    Hero1024,
    Hero2048,
}

impl TexturePageResolutionV22 {
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
pub enum TextureChannelV22 {
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
    MicroCavity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialClassV22 {
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
pub struct SurfaceTextureRecipeV22 {
    pub material_id: BeautyMaterialIdV22,
    pub surface_id: BeautySurfaceIdV22,
    pub class: MaterialClassV22,
    pub resolution: TexturePageResolutionV22,
    pub meters_per_tile: f32,
    pub channels: Vec<TextureChannelV22>,
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

impl SurfaceTextureRecipeV22 {
    pub fn new(
        surface_id: BeautySurfaceIdV22,
        material_id: BeautyMaterialIdV22,
        class: MaterialClassV22,
        meters_per_tile: f32,
    ) -> Self {
        Self {
            material_id,
            surface_id,
            class,
            resolution: TexturePageResolutionV22::Near512,
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
            procedural_warp_0_to_1: 0.018,
            smear_risk_0_to_1: 0.018,
            page_cache_priority_0_to_1: 0.50,
        }
    }

    pub fn with_channels(mut self, channels: &[TextureChannelV22]) -> Self {
        self.channels = channels.to_vec();
        self
    }

    pub fn with_resolution(mut self, resolution: TexturePageResolutionV22) -> Self {
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

    pub fn has_channel(&self, channel: TextureChannelV22) -> bool {
        self.channels.contains(&channel)
    }

    pub fn is_structured_enough(&self) -> bool {
        self.meters_per_tile > 0.05
            && self.has_channel(TextureChannelV22::BaseColor)
            && self.has_channel(TextureChannelV22::Normal)
            && self.has_channel(TextureChannelV22::Roughness)
            && self.has_channel(TextureChannelV22::AmbientOcclusion)
            && self.procedural_warp_0_to_1 <= 0.040
            && self.smear_risk_0_to_1 <= 0.045
            && self.normal_strength_0_to_1 > 0.08
            && self.roughness_variation_0_to_1 > 0.08
    }

    pub fn wet_asphalt(surface_id: BeautySurfaceIdV22, material_id: BeautyMaterialIdV22) -> Self {
        Self::new(surface_id, material_id, MaterialClassV22::WetAsphalt, 1.8)
            .with_surface_detail(0.88, 0.48, 0.80, 0.42)
            .with_weathering(0.72, 0.92, 0.34, 0.02)
            .with_artifact_limits(0.015, 0.020)
            .with_cache_priority(0.96)
            .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn dirty_concrete(
        surface_id: BeautySurfaceIdV22,
        material_id: BeautyMaterialIdV22,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV22::DirtyConcrete,
            1.15,
        )
        .with_surface_detail(0.82, 0.44, 0.76, 0.40)
        .with_weathering(0.80, 0.34, 0.42, 0.04)
        .with_artifact_limits(0.014, 0.020)
        .with_cache_priority(0.84)
        .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn soil_mud(surface_id: BeautySurfaceIdV22, material_id: BeautyMaterialIdV22) -> Self {
        Self::new(surface_id, material_id, MaterialClassV22::SoilMud, 0.80)
            .with_surface_detail(0.94, 0.84, 0.88, 0.58)
            .with_weathering(0.80, 0.70, 0.18, 0.92)
            .with_artifact_limits(0.020, 0.030)
            .with_cache_priority(0.82)
            .with_channels(&STANDARD_ORGANIC_SURFACE_CHANNELS)
    }

    pub fn stone(surface_id: BeautySurfaceIdV22, material_id: BeautyMaterialIdV22) -> Self {
        Self::new(surface_id, material_id, MaterialClassV22::StoneRock, 0.55)
            .with_surface_detail(0.90, 0.70, 0.74, 0.48)
            .with_weathering(0.62, 0.24, 0.50, 0.16)
            .with_artifact_limits(0.014, 0.020)
            .with_cache_priority(0.68)
            .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn plant_leaf(surface_id: BeautySurfaceIdV22, material_id: BeautyMaterialIdV22) -> Self {
        Self::new(surface_id, material_id, MaterialClassV22::PlantLeaf, 0.22)
            .with_surface_detail(0.64, 0.22, 0.56, 0.68)
            .with_weathering(0.24, 0.46, 0.04, 0.96)
            .with_artifact_limits(0.014, 0.020)
            .with_cache_priority(0.64)
            .with_channels(&[
                TextureChannelV22::BaseColor,
                TextureChannelV22::Normal,
                TextureChannelV22::Roughness,
                TextureChannelV22::AmbientOcclusion,
                TextureChannelV22::Wetness,
                TextureChannelV22::OrganicMask,
                TextureChannelV22::Subsurface,
                TextureChannelV22::MicroCavity,
            ])
    }

    pub fn landfill_plastic(
        surface_id: BeautySurfaceIdV22,
        material_id: BeautyMaterialIdV22,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV22::LandfillPlastic,
            0.72,
        )
        .with_surface_detail(0.64, 0.40, 0.84, 0.70)
        .with_weathering(0.92, 0.50, 0.16, 0.24)
        .with_artifact_limits(0.014, 0.022)
        .with_cache_priority(0.72)
        .with_channels(&[
            TextureChannelV22::BaseColor,
            TextureChannelV22::Normal,
            TextureChannelV22::Height,
            TextureChannelV22::Roughness,
            TextureChannelV22::AmbientOcclusion,
            TextureChannelV22::Dirt,
            TextureChannelV22::Wetness,
            TextureChannelV22::DecalMask,
        ])
    }

    pub fn human_skin(surface_id: BeautySurfaceIdV22, material_id: BeautyMaterialIdV22) -> Self {
        Self::new(surface_id, material_id, MaterialClassV22::HumanSkin, 0.18)
            .with_resolution(TexturePageResolutionV22::Hero1024)
            .with_surface_detail(0.58, 0.12, 0.50, 0.30)
            .with_weathering(0.08, 0.16, 0.02, 0.02)
            .with_artifact_limits(0.008, 0.014)
            .with_cache_priority(0.92)
            .with_channels(&[
                TextureChannelV22::BaseColor,
                TextureChannelV22::Normal,
                TextureChannelV22::Roughness,
                TextureChannelV22::AmbientOcclusion,
                TextureChannelV22::Wetness,
                TextureChannelV22::Subsurface,
                TextureChannelV22::MicroCavity,
            ])
    }

    pub fn clothing_fabric(
        surface_id: BeautySurfaceIdV22,
        material_id: BeautyMaterialIdV22,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV22::ClothingFabric,
            0.35,
        )
        .with_surface_detail(0.68, 0.26, 0.78, 0.44)
        .with_weathering(0.38, 0.22, 0.02, 0.05)
        .with_artifact_limits(0.010, 0.018)
        .with_cache_priority(0.76)
        .with_channels(&[
            TextureChannelV22::BaseColor,
            TextureChannelV22::Normal,
            TextureChannelV22::Height,
            TextureChannelV22::Roughness,
            TextureChannelV22::AmbientOcclusion,
            TextureChannelV22::Dirt,
            TextureChannelV22::Wetness,
            TextureChannelV22::MicroCavity,
        ])
    }

    pub fn car_paint(surface_id: BeautySurfaceIdV22, material_id: BeautyMaterialIdV22) -> Self {
        Self::new(surface_id, material_id, MaterialClassV22::CarPaint, 1.25)
            .with_surface_detail(0.30, 0.08, 0.34, 0.18)
            .with_weathering(0.34, 0.56, 0.03, 0.01)
            .with_artifact_limits(0.008, 0.014)
            .with_cache_priority(0.86)
            .with_channels(&[
                TextureChannelV22::BaseColor,
                TextureChannelV22::Normal,
                TextureChannelV22::Roughness,
                TextureChannelV22::AmbientOcclusion,
                TextureChannelV22::Dirt,
                TextureChannelV22::Wetness,
                TextureChannelV22::ClearCoat,
                TextureChannelV22::DecalMask,
            ])
    }
}

pub const STANDARD_HARD_SURFACE_CHANNELS: [TextureChannelV22; 10] = [
    TextureChannelV22::BaseColor,
    TextureChannelV22::Normal,
    TextureChannelV22::Height,
    TextureChannelV22::Roughness,
    TextureChannelV22::AmbientOcclusion,
    TextureChannelV22::Dirt,
    TextureChannelV22::Wetness,
    TextureChannelV22::CrackChip,
    TextureChannelV22::SootOilCorrosion,
    TextureChannelV22::MicroCavity,
];

pub const STANDARD_ORGANIC_SURFACE_CHANNELS: [TextureChannelV22; 9] = [
    TextureChannelV22::BaseColor,
    TextureChannelV22::Normal,
    TextureChannelV22::Height,
    TextureChannelV22::Roughness,
    TextureChannelV22::AmbientOcclusion,
    TextureChannelV22::Dirt,
    TextureChannelV22::Wetness,
    TextureChannelV22::OrganicMask,
    TextureChannelV22::MicroCavity,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProceduralMaterialSampleV22 {
    pub base_color_linear: [f32; 3],
    pub normal_xy: [f32; 2],
    pub height_0_to_1: f32,
    pub roughness_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub wetness_0_to_1: f32,
    pub structured_detail_score_0_to_1: f32,
}

/// Deterministic low-cost material sampler for tests/debug previews.
/// Production should replace this with generated GPU texture pages using the same recipe contract.
pub fn sample_material_v22(
    recipe: &SurfaceTextureRecipeV22,
    uv: [f32; 2],
    seed: u64,
) -> ProceduralMaterialSampleV22 {
    let n1 = hash01(seed ^ 0xA51F_F022, uv[0], uv[1]);
    let n2 = hash01(seed ^ 0xC0DE_5022, uv[0] * 2.0, uv[1] * 1.3);
    let n3 = hash01(seed ^ 0xD17A_2022, uv[0] * 5.0, uv[1] * 5.0);

    // Blend different scales. Do not warp UVs; smears usually begin with excessive warp.
    let detail = (0.24 * n1 + 0.32 * n2 + 0.44 * n3).clamp(0.0, 1.0);

    let class_tint = match recipe.class {
        MaterialClassV22::WetAsphalt => [0.050, 0.052, 0.054],
        MaterialClassV22::DirtyConcrete => [0.42, 0.39, 0.34],
        MaterialClassV22::SoilMud => [0.18, 0.115, 0.065],
        MaterialClassV22::StoneRock => [0.33, 0.32, 0.30],
        MaterialClassV22::PlantLeaf => [0.065, 0.22, 0.070],
        MaterialClassV22::LandfillPlastic => [0.30, 0.28, 0.25],
        MaterialClassV22::HumanSkin => [0.65, 0.43, 0.32],
        MaterialClassV22::ClothingFabric => [0.16, 0.17, 0.21],
        MaterialClassV22::CarPaint => [0.16, 0.18, 0.20],
        _ => [0.25, 0.24, 0.22],
    };

    let albedo_var = recipe.albedo_variation_0_to_1 * (detail - 0.5) * 0.30;

    ProceduralMaterialSampleV22 {
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
        structured_detail_score_0_to_1: (0.30 + detail * 0.70).clamp(0.0, 1.0),
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
