//! V18 generated texture/material-page contract.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautyMaterialIdV18(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeautySurfaceIdV18(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexturePageKindV18 {
    BaseColor,
    Normal,
    Height,
    Roughness,
    AmbientOcclusion,
    Dirt,
    Wetness,
    CrackChip,
    SootOilCorrosion,
    Decal,
    MaterialState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturePageResolutionV18 {
    Hero1024,
    Near512,
    Mid256,
    Far128,
    Impostor64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceTextureRecipeV18 {
    pub material_id: BeautyMaterialIdV18,
    pub surface_id: BeautySurfaceIdV18,
    pub meters_per_tile: f32,
    pub normal_strength_0_to_1: f32,
    pub roughness_variation_0_to_1: f32,
    pub dirt_0_to_1: f32,
    pub wetness_response_0_to_1: f32,
    pub crack_chip_0_to_1: f32,
    pub procedural_warp_0_to_1: f32,
    pub resolution: TexturePageResolutionV18,
}

impl SurfaceTextureRecipeV18 {
    pub fn wet_asphalt(surface_id: BeautySurfaceIdV18, material_id: BeautyMaterialIdV18) -> Self {
        Self {
            material_id,
            surface_id,
            meters_per_tile: 2.4,
            normal_strength_0_to_1: 0.62,
            roughness_variation_0_to_1: 0.82,
            dirt_0_to_1: 0.46,
            wetness_response_0_to_1: 0.88,
            crack_chip_0_to_1: 0.38,
            procedural_warp_0_to_1: 0.12,
            resolution: TexturePageResolutionV18::Near512,
        }
    }

    pub fn dirty_concrete(
        surface_id: BeautySurfaceIdV18,
        material_id: BeautyMaterialIdV18,
    ) -> Self {
        Self {
            material_id,
            surface_id,
            meters_per_tile: 3.0,
            normal_strength_0_to_1: 0.70,
            roughness_variation_0_to_1: 0.76,
            dirt_0_to_1: 0.74,
            wetness_response_0_to_1: 0.48,
            crack_chip_0_to_1: 0.54,
            procedural_warp_0_to_1: 0.10,
            resolution: TexturePageResolutionV18::Near512,
        }
    }

    pub fn car_paint(surface_id: BeautySurfaceIdV18, material_id: BeautyMaterialIdV18) -> Self {
        Self {
            material_id,
            surface_id,
            meters_per_tile: 1.2,
            normal_strength_0_to_1: 0.08,
            roughness_variation_0_to_1: 0.24,
            dirt_0_to_1: 0.22,
            wetness_response_0_to_1: 0.64,
            crack_chip_0_to_1: 0.04,
            procedural_warp_0_to_1: 0.02,
            resolution: TexturePageResolutionV18::Near512,
        }
    }

    pub fn avoids_smear(&self) -> bool {
        self.procedural_warp_0_to_1 <= 0.18 && self.meters_per_tile > 0.05
    }

    pub fn has_photoreal_detail(&self) -> bool {
        self.avoids_smear()
            && self.normal_strength_0_to_1 > 0.05
            && self.roughness_variation_0_to_1 > 0.15
            && (self.dirt_0_to_1 > 0.1
                || self.wetness_response_0_to_1 > 0.1
                || self.crack_chip_0_to_1 > 0.05)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexturePageRequestV18 {
    pub kind: TexturePageKindV18,
    pub recipe: SurfaceTextureRecipeV18,
    pub cache_key: u128,
}

impl TexturePageRequestV18 {
    pub fn page_set(recipe: SurfaceTextureRecipeV18, seed: u64) -> Vec<Self> {
        let kinds = [
            TexturePageKindV18::BaseColor,
            TexturePageKindV18::Normal,
            TexturePageKindV18::Height,
            TexturePageKindV18::Roughness,
            TexturePageKindV18::AmbientOcclusion,
            TexturePageKindV18::Dirt,
            TexturePageKindV18::Wetness,
            TexturePageKindV18::CrackChip,
            TexturePageKindV18::Decal,
            TexturePageKindV18::MaterialState,
        ];

        kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| Self {
                kind,
                recipe,
                cache_key: ((recipe.surface_id.0 as u128) << 64)
                    ^ ((recipe.material_id.0 as u128) << 16)
                    ^ ((seed as u128) << 1)
                    ^ i as u128
                    ^ 0xA5FA_1118_u128,
            })
            .collect()
    }
}
