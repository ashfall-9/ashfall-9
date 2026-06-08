//! Draw-list boundary contract for V22.
//!
//! This is where the renderer should reject screen-attached glitch primitives
//! before they reach the player-facing Beauty frame.

use super::artifacts::BeautyPrimitiveClassV22;

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyDrawCommandV22 {
    pub id: u64,
    pub class: BeautyPrimitiveClassV22,
    pub stable_world_space: bool,
    pub screen_space: bool,
    pub camera_facing: bool,
    pub material_structured: bool,
}

impl BeautyDrawCommandV22 {
    pub fn allowed_in_strict_beauty(&self) -> bool {
        self.class.allowed_in_strict_beauty()
            && self.stable_world_space
            && !self.screen_space
            && !self.camera_facing
            && self.material_structured
    }

    pub fn atmosphere(id: u64) -> Self {
        Self {
            id,
            class: BeautyPrimitiveClassV22::AtmospherePass,
            stable_world_space: true,
            screen_space: false,
            camera_facing: false,
            material_structured: true,
        }
    }

    pub fn world_mesh(id: u64) -> Self {
        Self {
            id,
            class: BeautyPrimitiveClassV22::WorldMesh,
            stable_world_space: true,
            screen_space: false,
            camera_facing: false,
            material_structured: true,
        }
    }

    pub fn coherent_human(id: u64) -> Self {
        Self {
            id,
            class: BeautyPrimitiveClassV22::CoherentHumanMesh,
            stable_world_space: true,
            screen_space: false,
            camera_facing: false,
            material_structured: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BeautyDrawListV22 {
    pub commands: Vec<BeautyDrawCommandV22>,
}

impl BeautyDrawListV22 {
    pub fn strict_valid(&self) -> bool {
        self.commands
            .iter()
            .all(BeautyDrawCommandV22::allowed_in_strict_beauty)
    }
}
