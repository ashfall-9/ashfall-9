//! Structured generated texture/material pages.
//!
//! These are not hand-painted skins. They are rebuildable generated material caches.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyMaterialIdV20(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautySurfaceIdV20(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexturePageResolutionV20 {
    Far128,
    Mid256,
    Near512,
    Hero1024,
    Hero2048,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureChannelV20 {
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
pub enum MaterialClassV20 {
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
pub struct SurfaceTextureRecipeV20 {
    pub material_id: BeautyMaterialIdV20,
    pub surface_id: BeautySurfaceIdV20,
    pub class: MaterialClassV20,
    pub resolution: TexturePageResolutionV20,
    pub meters_per_tile: f32,
    pub channels: Vec<TextureChannelV20>,
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
}

impl SurfaceTextureRecipeV20 {
    pub fn new(
        surface_id: BeautySurfaceIdV20,
        material_id: BeautyMaterialIdV20,
        class: MaterialClassV20,
        meters_per_tile: f32,
    ) -> Self {
        Self {
            material_id,
            surface_id,
            class,
            resolution: TexturePageResolutionV20::Near512,
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
            procedural_warp_0_to_1: 0.035,
            smear_risk_0_to_1: 0.04,
        }
    }

    pub fn with_channels(mut self, channels: &[TextureChannelV20]) -> Self {
        self.channels = channels.to_vec();
        self
    }

    pub fn with_resolution(mut self, resolution: TexturePageResolutionV20) -> Self {
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

    pub fn has_channel(&self, channel: TextureChannelV20) -> bool {
        self.channels.contains(&channel)
    }

    pub fn is_structured_enough(&self) -> bool {
        self.meters_per_tile > 0.05
            && self.has_channel(TextureChannelV20::BaseColor)
            && self.has_channel(TextureChannelV20::Normal)
            && self.has_channel(TextureChannelV20::Roughness)
            && self.procedural_warp_0_to_1 <= 0.10
            && self.smear_risk_0_to_1 <= 0.15
    }

    pub fn wet_asphalt(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::WetAsphalt, 1.8)
            .with_surface_detail(0.84, 0.44, 0.76, 0.40)
            .with_weathering(0.68, 0.92, 0.34, 0.02)
            .with_artifact_limits(0.035, 0.04)
            .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn dirty_concrete(
        surface_id: BeautySurfaceIdV20,
        material_id: BeautyMaterialIdV20,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV20::DirtyConcrete,
            1.15,
        )
        .with_surface_detail(0.78, 0.40, 0.72, 0.38)
        .with_weathering(0.74, 0.34, 0.38, 0.04)
        .with_artifact_limits(0.030, 0.04)
        .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn soil_mud(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::SoilMud, 0.80)
            .with_surface_detail(0.92, 0.82, 0.86, 0.58)
            .with_weathering(0.80, 0.70, 0.18, 0.92)
            .with_artifact_limits(0.045, 0.06)
            .with_channels(&STANDARD_ORGANIC_SURFACE_CHANNELS)
    }

    pub fn stone(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::StoneRock, 0.55)
            .with_surface_detail(0.86, 0.66, 0.70, 0.46)
            .with_weathering(0.60, 0.24, 0.48, 0.16)
            .with_artifact_limits(0.025, 0.03)
            .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn plant_leaf(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::PlantLeaf, 0.22)
            .with_surface_detail(0.60, 0.20, 0.54, 0.68)
            .with_weathering(0.24, 0.46, 0.04, 0.95)
            .with_artifact_limits(0.025, 0.03)
            .with_channels(&[
                TextureChannelV20::BaseColor,
                TextureChannelV20::Normal,
                TextureChannelV20::Roughness,
                TextureChannelV20::AmbientOcclusion,
                TextureChannelV20::Wetness,
                TextureChannelV20::OrganicMask,
            ])
    }

    pub fn landfill_plastic(
        surface_id: BeautySurfaceIdV20,
        material_id: BeautyMaterialIdV20,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV20::LandfillPlastic,
            0.72,
        )
        .with_surface_detail(0.58, 0.36, 0.80, 0.70)
        .with_weathering(0.88, 0.50, 0.16, 0.24)
        .with_artifact_limits(0.025, 0.04)
        .with_channels(&[
            TextureChannelV20::BaseColor,
            TextureChannelV20::Normal,
            TextureChannelV20::Height,
            TextureChannelV20::Roughness,
            TextureChannelV20::Dirt,
            TextureChannelV20::Wetness,
            TextureChannelV20::DecalMask,
        ])
    }

    pub fn rusted_metal(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::RustedMetal, 0.95)
            .with_surface_detail(0.88, 0.60, 0.82, 0.52)
            .with_weathering(0.72, 0.30, 0.46, 0.02)
            .with_artifact_limits(0.020, 0.035)
            .with_channels(&STANDARD_HARD_SURFACE_CHANNELS)
    }

    pub fn landfill_fabric(
        surface_id: BeautySurfaceIdV20,
        material_id: BeautyMaterialIdV20,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV20::LandfillFabric,
            0.42,
        )
        .with_surface_detail(0.64, 0.38, 0.88, 0.62)
        .with_weathering(0.90, 0.44, 0.08, 0.18)
        .with_artifact_limits(0.020, 0.035)
        .with_channels(&[
            TextureChannelV20::BaseColor,
            TextureChannelV20::Normal,
            TextureChannelV20::Height,
            TextureChannelV20::Roughness,
            TextureChannelV20::AmbientOcclusion,
            TextureChannelV20::Dirt,
            TextureChannelV20::Wetness,
            TextureChannelV20::DecalMask,
        ])
    }

    pub fn cardboard_paper(
        surface_id: BeautySurfaceIdV20,
        material_id: BeautyMaterialIdV20,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV20::CardboardPaper,
            0.55,
        )
        .with_surface_detail(0.54, 0.30, 0.92, 0.58)
        .with_weathering(0.82, 0.38, 0.18, 0.10)
        .with_artifact_limits(0.020, 0.035)
        .with_channels(&[
            TextureChannelV20::BaseColor,
            TextureChannelV20::Normal,
            TextureChannelV20::Height,
            TextureChannelV20::Roughness,
            TextureChannelV20::AmbientOcclusion,
            TextureChannelV20::Dirt,
            TextureChannelV20::Wetness,
            TextureChannelV20::DecalMask,
        ])
    }

    pub fn glass(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::Glass, 0.90)
            .with_surface_detail(0.24, 0.06, 0.34, 0.24)
            .with_weathering(0.22, 0.58, 0.03, 0.01)
            .with_artifact_limits(0.010, 0.020)
            .with_channels(&[
                TextureChannelV20::BaseColor,
                TextureChannelV20::Normal,
                TextureChannelV20::Roughness,
                TextureChannelV20::AmbientOcclusion,
                TextureChannelV20::Wetness,
                TextureChannelV20::ClearCoat,
            ])
    }

    pub fn rubber(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::Rubber, 0.38)
            .with_surface_detail(0.70, 0.34, 0.94, 0.28)
            .with_weathering(0.58, 0.34, 0.10, 0.02)
            .with_artifact_limits(0.014, 0.026)
            .with_channels(&[
                TextureChannelV20::BaseColor,
                TextureChannelV20::Normal,
                TextureChannelV20::Height,
                TextureChannelV20::Roughness,
                TextureChannelV20::Dirt,
                TextureChannelV20::Wetness,
            ])
    }

    pub fn human_skin(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::HumanSkin, 0.18)
            .with_resolution(TexturePageResolutionV20::Hero1024)
            .with_surface_detail(0.56, 0.12, 0.48, 0.30)
            .with_weathering(0.08, 0.16, 0.02, 0.02)
            .with_artifact_limits(0.012, 0.02)
            .with_channels(&[
                TextureChannelV20::BaseColor,
                TextureChannelV20::Normal,
                TextureChannelV20::Roughness,
                TextureChannelV20::AmbientOcclusion,
                TextureChannelV20::Wetness,
                TextureChannelV20::Subsurface,
            ])
    }

    pub fn clothing_fabric(
        surface_id: BeautySurfaceIdV20,
        material_id: BeautyMaterialIdV20,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV20::ClothingFabric,
            0.35,
        )
        .with_surface_detail(0.62, 0.22, 0.74, 0.42)
        .with_weathering(0.36, 0.22, 0.02, 0.05)
        .with_artifact_limits(0.018, 0.03)
        .with_channels(&[
            TextureChannelV20::BaseColor,
            TextureChannelV20::Normal,
            TextureChannelV20::Height,
            TextureChannelV20::Roughness,
            TextureChannelV20::AmbientOcclusion,
            TextureChannelV20::Dirt,
            TextureChannelV20::Wetness,
        ])
    }

    pub fn hair(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::Hair, 0.16)
            .with_resolution(TexturePageResolutionV20::Hero1024)
            .with_surface_detail(0.72, 0.18, 0.62, 0.34)
            .with_weathering(0.18, 0.32, 0.01, 0.04)
            .with_artifact_limits(0.012, 0.020)
            .with_channels(&[
                TextureChannelV20::BaseColor,
                TextureChannelV20::Normal,
                TextureChannelV20::Roughness,
                TextureChannelV20::AmbientOcclusion,
                TextureChannelV20::Wetness,
                TextureChannelV20::HairAnisotropy,
            ])
    }

    pub fn car_paint(surface_id: BeautySurfaceIdV20, material_id: BeautyMaterialIdV20) -> Self {
        Self::new(surface_id, material_id, MaterialClassV20::CarPaint, 1.25)
            .with_surface_detail(0.24, 0.08, 0.32, 0.18)
            .with_weathering(0.34, 0.56, 0.03, 0.01)
            .with_artifact_limits(0.012, 0.02)
            .with_channels(&[
                TextureChannelV20::BaseColor,
                TextureChannelV20::Normal,
                TextureChannelV20::Roughness,
                TextureChannelV20::Dirt,
                TextureChannelV20::Wetness,
                TextureChannelV20::ClearCoat,
                TextureChannelV20::DecalMask,
            ])
    }
}

pub const STANDARD_HARD_SURFACE_CHANNELS: [TextureChannelV20; 9] = [
    TextureChannelV20::BaseColor,
    TextureChannelV20::Normal,
    TextureChannelV20::Height,
    TextureChannelV20::Roughness,
    TextureChannelV20::AmbientOcclusion,
    TextureChannelV20::Dirt,
    TextureChannelV20::Wetness,
    TextureChannelV20::CrackChip,
    TextureChannelV20::SootOilCorrosion,
];

pub const STANDARD_ORGANIC_SURFACE_CHANNELS: [TextureChannelV20; 8] = [
    TextureChannelV20::BaseColor,
    TextureChannelV20::Normal,
    TextureChannelV20::Height,
    TextureChannelV20::Roughness,
    TextureChannelV20::AmbientOcclusion,
    TextureChannelV20::Dirt,
    TextureChannelV20::Wetness,
    TextureChannelV20::OrganicMask,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProceduralMaterialSampleV20 {
    pub base_color_linear: [f32; 3],
    pub normal_xy: [f32; 2],
    pub height_0_to_1: f32,
    pub roughness_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub wetness_0_to_1: f32,
}

/// Deterministic low-cost material sampler for early CPU/debug previews.
/// Production can replace this with Vulkan compute using the same recipe contract.
pub fn sample_material_v20(
    recipe: &SurfaceTextureRecipeV20,
    uv_meters: [f32; 2],
    seed: u64,
) -> ProceduralMaterialSampleV20 {
    let scale = (1.0 / recipe.meters_per_tile.max(0.05)).max(0.01);
    let u = uv_meters[0] * scale;
    let v = uv_meters[1] * scale;
    let n1 = value_noise_2d(u, v, seed);
    let n2 = value_noise_2d(u * 3.1 + 17.0, v * 3.1 - 11.0, seed ^ 0xA5A5_2020);
    let n3 = value_noise_2d(u * 9.0 - 2.0, v * 9.0 + 5.0, seed ^ 0x7A11_0020);
    let n4 = value_noise_2d(u * 19.0 + 3.7, v * 17.0 - 8.1, seed ^ 0xD37A_2020);
    let structured = (n1 * 0.46 + n2 * 0.28 + n3 * 0.18 + n4 * 0.08).clamp(0.0, 1.0);
    let crack_threshold = (0.94 - recipe.crack_chip_0_to_1 * 0.52).clamp(0.38, 0.98);
    let crack_mask = ((n4 - crack_threshold) / (1.0 - crack_threshold).max(0.02)).clamp(0.0, 1.0);
    let height = ((structured * 0.78 + n4 * 0.22) * recipe.height_strength_0_to_1
        + crack_mask * recipe.crack_chip_0_to_1 * 0.18)
        .clamp(0.0, 1.0);
    let roughness =
        (0.42 + structured * recipe.roughness_variation_0_to_1 * 0.44 + crack_mask * 0.12)
            .clamp(0.04, 0.98);
    let dirt = (recipe.dirt_0_to_1 * (0.35 + 0.65 * n2)).clamp(0.0, 1.0);
    let wetness = (recipe.wetness_response_0_to_1 * (0.25 + 0.75 * (1.0 - n3))).clamp(0.0, 1.0);
    let mut base = base_color_for_class(recipe.class, structured, dirt, wetness);
    let albedo_detail = 1.0 + (n4 - 0.5) * recipe.albedo_variation_0_to_1 * 0.42
        - crack_mask * recipe.crack_chip_0_to_1 * 0.36
        - dirt * 0.08;
    for channel in &mut base {
        *channel = (*channel * albedo_detail).clamp(0.0, 1.0);
    }

    ProceduralMaterialSampleV20 {
        base_color_linear: base,
        normal_xy: [
            ((n2 - 0.5) * 0.74 + (n4 - 0.5) * 0.26) * recipe.normal_strength_0_to_1,
            ((n3 - 0.5) * 0.74 + (n4 - 0.5) * 0.26) * recipe.normal_strength_0_to_1,
        ],
        height_0_to_1: height,
        roughness_0_to_1: roughness,
        dirt_0_to_1: dirt,
        wetness_0_to_1: wetness,
    }
}

fn base_color_for_class(
    class: MaterialClassV20,
    variation: f32,
    dirt: f32,
    wetness: f32,
) -> [f32; 3] {
    let base = match class {
        MaterialClassV20::WetAsphalt => [0.030, 0.034, 0.036],
        MaterialClassV20::DirtyConcrete => [0.42, 0.40, 0.36],
        MaterialClassV20::SoilMud => [0.20, 0.13, 0.075],
        MaterialClassV20::StoneRock => [0.36, 0.35, 0.33],
        MaterialClassV20::PlantLeaf => [0.085, 0.24, 0.065],
        MaterialClassV20::BarkWood => [0.24, 0.13, 0.075],
        MaterialClassV20::RustedMetal => [0.38, 0.17, 0.07],
        MaterialClassV20::LandfillPlastic => [0.33, 0.34, 0.30],
        MaterialClassV20::LandfillFabric => [0.25, 0.22, 0.20],
        MaterialClassV20::CardboardPaper => [0.44, 0.33, 0.21],
        MaterialClassV20::Glass => [0.62, 0.72, 0.78],
        MaterialClassV20::CarPaint => [0.15, 0.18, 0.20],
        MaterialClassV20::Rubber => [0.015, 0.015, 0.014],
        MaterialClassV20::HumanSkin => [0.62, 0.40, 0.30],
        MaterialClassV20::ClothingFabric => [0.09, 0.10, 0.14],
        MaterialClassV20::Hair => [0.06, 0.045, 0.035],
    };
    let var = (variation - 0.5) * 0.16;
    let dirt_factor = 1.0 - dirt * 0.32;
    let wet_factor = 1.0 - wetness * 0.10;
    [
        (base[0] + var).max(0.0) * dirt_factor * wet_factor,
        (base[1] + var).max(0.0) * dirt_factor * wet_factor,
        (base[2] + var).max(0.0) * dirt_factor * wet_factor,
    ]
}

fn value_noise_2d(x: f32, y: f32, seed: u64) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - xi as f32;
    let yf = y - yi as f32;
    let sx = smoothstep(xf);
    let sy = smoothstep(yf);
    let a = hash01(xi, yi, seed);
    let b = hash01(xi + 1, yi, seed);
    let c = hash01(xi, yi + 1, seed);
    let d = hash01(xi + 1, yi + 1, seed);
    let ab = lerp(a, b, sx);
    let cd = lerp(c, d, sx);
    lerp(ab, cd, sy)
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hash01(x: i32, y: i32, seed: u64) -> f32 {
    let mut n = seed ^ ((x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    n ^= (y as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    n ^= n >> 30;
    n = n.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94D0_49BB_1331_11EB);
    n ^= n >> 31;
    ((n & 0x00FF_FFFF) as f32) / 16_777_215.0
}
