//! Generated material page requests.
//!
//! "Textureless" means Ashfall should not rely mainly on hand-painted unique
//! texture skins. It does not mean flat colors. Generated pages are rebuildable
//! runtime/asset-cache data for physical surface detail.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeautyMaterialIdV16(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeautySurfaceIdV16(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialPageChannelV16 {
    BaseColor,
    Normal,
    Height,
    Roughness,
    Metallic,
    DirtMask,
    WetnessMask,
    CrackChipMask,
    SootOilCorrosionMask,
    MaterialStateMask,
    DecalMask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialPageResolutionV16 {
    R256,
    R512,
    R1024,
    R2048,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialPageRequestV16 {
    pub surface_id: BeautySurfaceIdV16,
    pub material_id: BeautyMaterialIdV16,
    pub channels: Vec<MaterialPageChannelV16>,
    pub resolution: MaterialPageResolutionV16,
    pub seed: u64,
    pub allow_runtime_regeneration: bool,
}

impl MaterialPageRequestV16 {
    pub fn wet_asphalt(
        surface_id: BeautySurfaceIdV16,
        material_id: BeautyMaterialIdV16,
        seed: u64,
    ) -> Self {
        Self {
            surface_id,
            material_id,
            channels: vec![
                MaterialPageChannelV16::BaseColor,
                MaterialPageChannelV16::Normal,
                MaterialPageChannelV16::Height,
                MaterialPageChannelV16::Roughness,
                MaterialPageChannelV16::DirtMask,
                MaterialPageChannelV16::WetnessMask,
                MaterialPageChannelV16::CrackChipMask,
                MaterialPageChannelV16::MaterialStateMask,
            ],
            resolution: MaterialPageResolutionV16::R1024,
            seed,
            allow_runtime_regeneration: true,
        }
    }

    pub fn dirty_concrete(
        surface_id: BeautySurfaceIdV16,
        material_id: BeautyMaterialIdV16,
        seed: u64,
    ) -> Self {
        Self {
            surface_id,
            material_id,
            channels: vec![
                MaterialPageChannelV16::BaseColor,
                MaterialPageChannelV16::Normal,
                MaterialPageChannelV16::Height,
                MaterialPageChannelV16::Roughness,
                MaterialPageChannelV16::DirtMask,
                MaterialPageChannelV16::WetnessMask,
                MaterialPageChannelV16::CrackChipMask,
                MaterialPageChannelV16::SootOilCorrosionMask,
                MaterialPageChannelV16::DecalMask,
            ],
            resolution: MaterialPageResolutionV16::R1024,
            seed,
            allow_runtime_regeneration: true,
        }
    }

    pub fn factory_vehicle(
        surface_id: BeautySurfaceIdV16,
        material_id: BeautyMaterialIdV16,
        seed: u64,
    ) -> Self {
        Self {
            surface_id,
            material_id,
            channels: vec![
                MaterialPageChannelV16::BaseColor,
                MaterialPageChannelV16::Normal,
                MaterialPageChannelV16::Roughness,
                MaterialPageChannelV16::DirtMask,
                MaterialPageChannelV16::WetnessMask,
                MaterialPageChannelV16::DecalMask,
            ],
            resolution: MaterialPageResolutionV16::R1024,
            seed,
            allow_runtime_regeneration: true,
        }
    }

    pub fn has_microdetail(&self) -> bool {
        self.channels.iter().any(|channel| {
            matches!(
                channel,
                MaterialPageChannelV16::Normal
                    | MaterialPageChannelV16::Height
                    | MaterialPageChannelV16::Roughness
                    | MaterialPageChannelV16::DirtMask
                    | MaterialPageChannelV16::WetnessMask
                    | MaterialPageChannelV16::CrackChipMask
                    | MaterialPageChannelV16::SootOilCorrosionMask
            )
        })
    }
}
