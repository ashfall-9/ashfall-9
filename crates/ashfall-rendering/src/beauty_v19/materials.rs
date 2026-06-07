//! Structured generated texture page contract.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyMaterialIdV19(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautySurfaceIdV19(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexturePageResolutionV19 {
    Far128,
    Mid256,
    Near512,
    Hero1024,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureChannelV19 {
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialClassV19 {
    WetAsphalt,
    DirtyConcrete,
    SoilMud,
    StoneRock,
    PlantLeaf,
    BarkWood,
    RustedMetal,
    LandfillPlastic,
    LandfillFabric,
    Glass,
    CarPaint,
    Rubber,
    HumanSkin,
    ClothingFabric,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceTextureRecipeV19 {
    pub material_id: BeautyMaterialIdV19,
    pub surface_id: BeautySurfaceIdV19,
    pub class: MaterialClassV19,
    pub resolution: TexturePageResolutionV19,
    pub meters_per_tile: f32,
    pub channels: Vec<TextureChannelV19>,
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

impl SurfaceTextureRecipeV19 {
    pub fn wet_asphalt(surface_id: BeautySurfaceIdV19, material_id: BeautyMaterialIdV19) -> Self {
        Self::new(surface_id, material_id, MaterialClassV19::WetAsphalt, 1.8)
            .with_surface_detail(0.82, 0.42, 0.72, 0.36)
            .with_weathering(0.64, 0.90, 0.30, 0.02)
            .with_channels(&[
                TextureChannelV19::BaseColor,
                TextureChannelV19::Normal,
                TextureChannelV19::Height,
                TextureChannelV19::Roughness,
                TextureChannelV19::AmbientOcclusion,
                TextureChannelV19::Dirt,
                TextureChannelV19::Wetness,
                TextureChannelV19::CrackChip,
                TextureChannelV19::SootOilCorrosion,
            ])
    }

    pub fn soil_mud(surface_id: BeautySurfaceIdV19, material_id: BeautyMaterialIdV19) -> Self {
        Self::new(surface_id, material_id, MaterialClassV19::SoilMud, 0.95)
            .with_surface_detail(0.90, 0.76, 0.85, 0.62)
            .with_weathering(0.78, 0.66, 0.22, 0.88)
            .with_channels(&[
                TextureChannelV19::BaseColor,
                TextureChannelV19::Normal,
                TextureChannelV19::Height,
                TextureChannelV19::Roughness,
                TextureChannelV19::AmbientOcclusion,
                TextureChannelV19::Dirt,
                TextureChannelV19::Wetness,
                TextureChannelV19::OrganicMask,
            ])
    }

    pub fn stone(surface_id: BeautySurfaceIdV19, material_id: BeautyMaterialIdV19) -> Self {
        Self::new(surface_id, material_id, MaterialClassV19::StoneRock, 0.72)
            .with_surface_detail(0.88, 0.62, 0.70, 0.48)
            .with_weathering(0.56, 0.26, 0.42, 0.18)
            .with_channels(&[
                TextureChannelV19::BaseColor,
                TextureChannelV19::Normal,
                TextureChannelV19::Height,
                TextureChannelV19::Roughness,
                TextureChannelV19::AmbientOcclusion,
                TextureChannelV19::Dirt,
                TextureChannelV19::CrackChip,
            ])
    }

    pub fn plant_leaf(surface_id: BeautySurfaceIdV19, material_id: BeautyMaterialIdV19) -> Self {
        Self::new(surface_id, material_id, MaterialClassV19::PlantLeaf, 0.28)
            .with_surface_detail(0.52, 0.18, 0.48, 0.74)
            .with_weathering(0.24, 0.42, 0.06, 0.92)
            .with_channels(&[
                TextureChannelV19::BaseColor,
                TextureChannelV19::Normal,
                TextureChannelV19::Roughness,
                TextureChannelV19::AmbientOcclusion,
                TextureChannelV19::Wetness,
                TextureChannelV19::OrganicMask,
            ])
    }

    pub fn landfill_plastic(
        surface_id: BeautySurfaceIdV19,
        material_id: BeautyMaterialIdV19,
    ) -> Self {
        Self::new(
            surface_id,
            material_id,
            MaterialClassV19::LandfillPlastic,
            0.80,
        )
        .with_surface_detail(0.55, 0.32, 0.80, 0.72)
        .with_weathering(0.86, 0.54, 0.18, 0.24)
        .with_channels(&[
            TextureChannelV19::BaseColor,
            TextureChannelV19::Normal,
            TextureChannelV19::Height,
            TextureChannelV19::Roughness,
            TextureChannelV19::Dirt,
            TextureChannelV19::Wetness,
            TextureChannelV19::DecalMask,
        ])
    }

    pub fn car_paint(surface_id: BeautySurfaceIdV19, material_id: BeautyMaterialIdV19) -> Self {
        Self::new(surface_id, material_id, MaterialClassV19::CarPaint, 1.25)
            .with_surface_detail(0.28, 0.10, 0.34, 0.20)
            .with_weathering(0.32, 0.58, 0.04, 0.02)
            .with_channels(&[
                TextureChannelV19::BaseColor,
                TextureChannelV19::Normal,
                TextureChannelV19::Roughness,
                TextureChannelV19::Dirt,
                TextureChannelV19::Wetness,
                TextureChannelV19::ClearCoat,
                TextureChannelV19::DecalMask,
            ])
    }

    pub fn new(
        surface_id: BeautySurfaceIdV19,
        material_id: BeautyMaterialIdV19,
        class: MaterialClassV19,
        meters_per_tile: f32,
    ) -> Self {
        Self {
            material_id,
            surface_id,
            class,
            resolution: TexturePageResolutionV19::Near512,
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
            procedural_warp_0_to_1: 0.04,
            smear_risk_0_to_1: 0.05,
        }
    }

    pub fn with_channels(mut self, channels: &[TextureChannelV19]) -> Self {
        self.channels = channels.to_vec();
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

    pub fn has_channel(&self, channel: TextureChannelV19) -> bool {
        self.channels.contains(&channel)
    }

    pub fn is_structured_enough(&self) -> bool {
        self.meters_per_tile > 0.05
            && self.has_channel(TextureChannelV19::BaseColor)
            && self.has_channel(TextureChannelV19::Normal)
            && self.has_channel(TextureChannelV19::Roughness)
            && self.procedural_warp_0_to_1 <= 0.12
            && self.smear_risk_0_to_1 <= 0.20
    }
}
