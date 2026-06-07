//! Draw contract for artifact-free Beauty passes.

use super::geometry::BoundsV19;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BeautyPassKindV19 {
    SkyAtmosphere,
    CloudLayer,
    DirectionalLight,
    WorldOpaqueGeometry,
    WorldMaskedFoliage,
    GroundedWaterFilms,
    DecalsAndDirt,
    Humans,
    Vehicles,
    RestrainedBloom,
    DebugOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BeautyPrimitiveClassV19 {
    WorldMesh,
    TerrainPatch,
    CurveMesh,
    FoliageInstance,
    LandfillProp,
    HumanProxy,
    VehicleProxy,
    GroundedWaterFilm,
    AtmospherePass,
    CloudLayerPass,
    ScreenSpaceStripeForbidden,
    CameraFacingCircleForbidden,
    CameraFacingEllipseForbidden,
    CircularGlareSpriteForbidden,
    DebugBoxForbidden,
    DebugLineForbidden,
}

impl BeautyPrimitiveClassV19 {
    pub fn is_forbidden_in_beauty(self) -> bool {
        matches!(
            self,
            Self::ScreenSpaceStripeForbidden
                | Self::CameraFacingCircleForbidden
                | Self::CameraFacingEllipseForbidden
                | Self::CircularGlareSpriteForbidden
                | Self::DebugBoxForbidden
                | Self::DebugLineForbidden
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyDrawItemV19 {
    pub pass: BeautyPassKindV19,
    pub class: BeautyPrimitiveClassV19,
    pub bounds: Option<BoundsV19>,
    pub estimated_triangles: u32,
    pub estimated_instances: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BeautyDrawListV19 {
    pub items: Vec<BeautyDrawItemV19>,
}

impl BeautyDrawListV19 {
    pub fn forbidden_item_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.class.is_forbidden_in_beauty())
            .count()
    }

    pub fn estimated_triangles(&self) -> u64 {
        self.items
            .iter()
            .map(|item| item.estimated_triangles as u64)
            .sum()
    }

    pub fn estimated_instances(&self) -> u64 {
        self.items
            .iter()
            .map(|item| item.estimated_instances as u64)
            .sum()
    }
}
