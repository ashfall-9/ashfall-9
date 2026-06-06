//! Procedural material page contract for V17 Beauty Mode.
//!
//! "Textureless" means not relying on hand-painted unique texture skins.
//! It does not mean flat colors. Beauty Mode needs generated/cacheable pages
//! for normals, height, roughness, dirt, wetness, cracks, soot, oil, corrosion,
//! decals, and material state.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeautyMaterialIdV17(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeautySurfaceIdV17(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MaterialPageKindV17 {
    BaseColor,
    Normal,
    Height,
    Roughness,
    AmbientOcclusion,
    DirtMask,
    WetnessMask,
    CrackChipMask,
    SootOilCorrosionMask,
    DecalAtlas,
    MaterialState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaterialResolutionClassV17 {
    Hero1024,
    Near512,
    Mid256,
    Far128,
    Impostor64,
}

impl MaterialResolutionClassV17 {
    pub fn texel_size(&self) -> u32 {
        match self {
            Self::Hero1024 => 1024,
            Self::Near512 => 512,
            Self::Mid256 => 256,
            Self::Far128 => 128,
            Self::Impostor64 => 64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialMicrodetailV17 {
    pub has_surface_grain: bool,
    pub has_height_variation: bool,
    pub has_roughness_variation: bool,
    pub has_edge_wear: bool,
    pub has_dirt_accumulation: bool,
    pub has_wetness_response: bool,
    pub has_cracks_or_chips: bool,
    pub procedural_node_budget: u16,
    pub world_meters_per_tile: f32,
}

impl MaterialMicrodetailV17 {
    pub fn wet_asphalt() -> Self {
        Self {
            has_surface_grain: true,
            has_height_variation: true,
            has_roughness_variation: true,
            has_edge_wear: false,
            has_dirt_accumulation: true,
            has_wetness_response: true,
            has_cracks_or_chips: true,
            procedural_node_budget: 18,
            world_meters_per_tile: 2.6,
        }
    }

    pub fn dirty_concrete() -> Self {
        Self {
            has_surface_grain: true,
            has_height_variation: true,
            has_roughness_variation: true,
            has_edge_wear: true,
            has_dirt_accumulation: true,
            has_wetness_response: true,
            has_cracks_or_chips: true,
            procedural_node_budget: 22,
            world_meters_per_tile: 3.4,
        }
    }

    pub fn car_paint() -> Self {
        Self {
            has_surface_grain: false,
            has_height_variation: false,
            has_roughness_variation: true,
            has_edge_wear: true,
            has_dirt_accumulation: true,
            has_wetness_response: true,
            has_cracks_or_chips: false,
            procedural_node_budget: 12,
            world_meters_per_tile: 1.4,
        }
    }

    pub fn is_photoreal_candidate(&self) -> bool {
        self.procedural_node_budget >= 8
            && self.world_meters_per_tile > 0.05
            && self.has_roughness_variation
            && (self.has_surface_grain || self.has_dirt_accumulation || self.has_edge_wear)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialPageRequestV17 {
    pub surface_id: BeautySurfaceIdV17,
    pub material_id: BeautyMaterialIdV17,
    pub kind: MaterialPageKindV17,
    pub resolution_class: MaterialResolutionClassV17,
    pub seed: u64,
    pub priority_0_to_1: f32,
    pub microdetail: MaterialMicrodetailV17,
    pub persistent_cache_key: u128,
}

impl MaterialPageRequestV17 {
    pub fn new(
        surface_id: BeautySurfaceIdV17,
        material_id: BeautyMaterialIdV17,
        kind: MaterialPageKindV17,
        resolution_class: MaterialResolutionClassV17,
        seed: u64,
        microdetail: MaterialMicrodetailV17,
    ) -> Self {
        Self {
            surface_id,
            material_id,
            kind,
            resolution_class,
            seed,
            priority_0_to_1: 0.7,
            microdetail,
            persistent_cache_key: cache_key(surface_id, material_id, kind, seed),
        }
    }

    pub fn has_microdetail(&self) -> bool {
        matches!(
            self.kind,
            MaterialPageKindV17::Normal
                | MaterialPageKindV17::Height
                | MaterialPageKindV17::Roughness
                | MaterialPageKindV17::DirtMask
                | MaterialPageKindV17::WetnessMask
                | MaterialPageKindV17::CrackChipMask
                | MaterialPageKindV17::SootOilCorrosionMask
                | MaterialPageKindV17::MaterialState
        ) && self.microdetail.is_photoreal_candidate()
    }

    pub fn wet_asphalt_pack(
        surface_id: BeautySurfaceIdV17,
        material_id: BeautyMaterialIdV17,
        seed: u64,
    ) -> Vec<Self> {
        let detail = MaterialMicrodetailV17::wet_asphalt();
        material_pack(
            surface_id,
            material_id,
            seed,
            detail,
            MaterialResolutionClassV17::Near512,
            &[
                MaterialPageKindV17::BaseColor,
                MaterialPageKindV17::Normal,
                MaterialPageKindV17::Height,
                MaterialPageKindV17::Roughness,
                MaterialPageKindV17::DirtMask,
                MaterialPageKindV17::WetnessMask,
                MaterialPageKindV17::CrackChipMask,
                MaterialPageKindV17::MaterialState,
            ],
        )
    }

    pub fn dirty_concrete_pack(
        surface_id: BeautySurfaceIdV17,
        material_id: BeautyMaterialIdV17,
        seed: u64,
    ) -> Vec<Self> {
        let detail = MaterialMicrodetailV17::dirty_concrete();
        material_pack(
            surface_id,
            material_id,
            seed,
            detail,
            MaterialResolutionClassV17::Near512,
            &[
                MaterialPageKindV17::BaseColor,
                MaterialPageKindV17::Normal,
                MaterialPageKindV17::Height,
                MaterialPageKindV17::Roughness,
                MaterialPageKindV17::AmbientOcclusion,
                MaterialPageKindV17::DirtMask,
                MaterialPageKindV17::WetnessMask,
                MaterialPageKindV17::CrackChipMask,
                MaterialPageKindV17::DecalAtlas,
            ],
        )
    }
}

fn material_pack(
    surface_id: BeautySurfaceIdV17,
    material_id: BeautyMaterialIdV17,
    seed: u64,
    detail: MaterialMicrodetailV17,
    resolution_class: MaterialResolutionClassV17,
    kinds: &[MaterialPageKindV17],
) -> Vec<MaterialPageRequestV17> {
    kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            MaterialPageRequestV17::new(
                surface_id,
                material_id,
                *kind,
                resolution_class,
                seed ^ ((index as u64 + 1) * 0x9E37_79B9),
                detail,
            )
        })
        .collect()
}

fn cache_key(
    surface_id: BeautySurfaceIdV17,
    material_id: BeautyMaterialIdV17,
    kind: MaterialPageKindV17,
    seed: u64,
) -> u128 {
    let kind_code = kind as u128;
    ((surface_id.0 as u128) << 64)
        ^ ((material_id.0 as u128) << 16)
        ^ ((seed as u128) << 1)
        ^ kind_code
        ^ 0xA5FA_1100_0000_0017_u128
}
