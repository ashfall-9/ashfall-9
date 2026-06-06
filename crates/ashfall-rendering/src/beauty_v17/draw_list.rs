//! Renderer-facing draw-list bridge for V17 Beauty Mode.
//!
//! This is still data-only. Codex should translate these commands into the
//! existing window renderer or RenderPacket path. It deliberately does not expose
//! Vulkan/Vulkano/winit types.

use super::contract::BeautySceneV17;
use super::geometry::{BeautyBoundsV17, BeautyObjectIdV17};
use super::material_pages::{BeautyMaterialIdV17, BeautySurfaceIdV17};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeautyPrimitiveKindV17 {
    SkyDome,
    CloudLayer,
    SunDisk,
    MoonDisk,
    RoadMesh,
    CurbMesh,
    FacadeMesh,
    CurveTube,
    PuddleSurface,
    ScatterField,
    HumanProxy,
    VehicleProxy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeautyDrawCommandV17 {
    pub object_id: Option<BeautyObjectIdV17>,
    pub kind: BeautyPrimitiveKindV17,
    pub bounds: Option<BeautyBoundsV17>,
    pub material_id: Option<BeautyMaterialIdV17>,
    pub surface_id: Option<BeautySurfaceIdV17>,
    pub world_points: Vec<[f32; 3]>,
    pub scale_or_radius_meters: f32,
    pub detail_priority_0_to_1: f32,
    pub identity_locked: bool,
}

impl BeautyDrawCommandV17 {
    pub fn sky(kind: BeautyPrimitiveKindV17, priority: f32) -> Self {
        Self {
            object_id: None,
            kind,
            bounds: None,
            material_id: None,
            surface_id: None,
            world_points: Vec::new(),
            scale_or_radius_meters: 1.0,
            detail_priority_0_to_1: priority,
            identity_locked: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BeautyDrawListV17 {
    pub frame_index: u64,
    pub commands: Vec<BeautyDrawCommandV17>,
}

impl BeautyDrawListV17 {
    pub fn from_scene(scene: &BeautySceneV17) -> Self {
        let mut list = BeautyDrawListV17 {
            frame_index: scene.frame_index,
            commands: Vec::new(),
        };

        if scene.environment.has_proper_sky() {
            list.commands.push(BeautyDrawCommandV17::sky(
                BeautyPrimitiveKindV17::SkyDome,
                1.0,
            ));
            list.commands.push(BeautyDrawCommandV17::sky(
                BeautyPrimitiveKindV17::CloudLayer,
                0.8,
            ));
            if scene.environment.sun_intensity_lux > 0.0 {
                list.commands.push(BeautyDrawCommandV17::sky(
                    BeautyPrimitiveKindV17::SunDisk,
                    0.9,
                ));
            }
            if scene.environment.moon_intensity_lux > 0.0 {
                list.commands.push(BeautyDrawCommandV17::sky(
                    BeautyPrimitiveKindV17::MoonDisk,
                    0.7,
                ));
            }
        }

        for cell in &scene.cells {
            for road in &cell.roads {
                list.commands.push(BeautyDrawCommandV17 {
                    object_id: Some(road.object_id),
                    kind: BeautyPrimitiveKindV17::RoadMesh,
                    bounds: Some(cell.bounds),
                    material_id: Some(road.material_id),
                    surface_id: Some(road.surface_id),
                    world_points: road.centerline_world.clone(),
                    scale_or_radius_meters: road.width_meters,
                    detail_priority_0_to_1: 1.0,
                    identity_locked: true,
                });
            }
            for curb in &cell.curbs {
                list.commands.push(BeautyDrawCommandV17 {
                    object_id: Some(curb.object_id),
                    kind: BeautyPrimitiveKindV17::CurbMesh,
                    bounds: Some(cell.bounds),
                    material_id: Some(curb.material_id),
                    surface_id: None,
                    world_points: vec![curb.start_world, curb.end_world],
                    scale_or_radius_meters: curb.width_meters,
                    detail_priority_0_to_1: 0.86,
                    identity_locked: true,
                });
            }
            for facade in &cell.facades {
                list.commands.push(BeautyDrawCommandV17 {
                    object_id: Some(facade.object_id),
                    kind: BeautyPrimitiveKindV17::FacadeMesh,
                    bounds: Some(facade.bounds),
                    material_id: Some(facade.material_id),
                    surface_id: None,
                    world_points: vec![facade.bounds.center()],
                    scale_or_radius_meters: facade.bevel_radius_meters,
                    detail_priority_0_to_1: 0.82,
                    identity_locked: true,
                });
            }
            for curve in &cell.pipes_and_cables {
                list.commands.push(BeautyDrawCommandV17 {
                    object_id: Some(curve.object_id),
                    kind: BeautyPrimitiveKindV17::CurveTube,
                    bounds: Some(cell.bounds),
                    material_id: Some(curve.material_id),
                    surface_id: None,
                    world_points: curve.points_world.clone(),
                    scale_or_radius_meters: curve.radius_meters,
                    detail_priority_0_to_1: 0.66,
                    identity_locked: true,
                });
            }
            for puddle in &cell.puddles {
                list.commands.push(BeautyDrawCommandV17 {
                    object_id: Some(puddle.object_id),
                    kind: BeautyPrimitiveKindV17::PuddleSurface,
                    bounds: Some(cell.bounds),
                    material_id: Some(puddle.material_id),
                    surface_id: Some(puddle.receiver_surface_id),
                    world_points: vec![puddle.center_world],
                    scale_or_radius_meters: puddle.radius_meters,
                    detail_priority_0_to_1: 0.74,
                    identity_locked: false,
                });
            }
            for scatter in &cell.scatter_fields {
                list.commands.push(BeautyDrawCommandV17 {
                    object_id: Some(scatter.object_id),
                    kind: BeautyPrimitiveKindV17::ScatterField,
                    bounds: Some(scatter.bounds),
                    material_id: None,
                    surface_id: None,
                    world_points: vec![scatter.bounds.center()],
                    scale_or_radius_meters: scatter.density_0_to_1,
                    detail_priority_0_to_1: 0.48,
                    identity_locked: false,
                });
            }
        }

        for human in &scene.humans {
            list.commands.push(BeautyDrawCommandV17 {
                object_id: Some(BeautyObjectIdV17(human.entity_id)),
                kind: BeautyPrimitiveKindV17::HumanProxy,
                bounds: None,
                material_id: Some(human.clothing_material_id),
                surface_id: None,
                world_points: vec![human.position_world],
                scale_or_radius_meters: human.height_meters,
                detail_priority_0_to_1: 1.0,
                identity_locked: true,
            });
        }

        for vehicle in &scene.vehicles {
            list.commands.push(BeautyDrawCommandV17 {
                object_id: Some(BeautyObjectIdV17(vehicle.entity_id)),
                kind: BeautyPrimitiveKindV17::VehicleProxy,
                bounds: None,
                material_id: Some(vehicle.body_material_id),
                surface_id: None,
                world_points: vec![vehicle.position_world],
                scale_or_radius_meters: vehicle.length_meters,
                detail_priority_0_to_1: 0.92,
                identity_locked: true,
            });
        }

        list
    }

    pub fn has_sky_commands(&self) -> bool {
        self.commands
            .iter()
            .any(|cmd| cmd.kind == BeautyPrimitiveKindV17::SkyDome)
    }

    pub fn has_world_anchored_human_and_vehicle(&self) -> bool {
        self.commands
            .iter()
            .any(|cmd| cmd.kind == BeautyPrimitiveKindV17::HumanProxy)
            && self
                .commands
                .iter()
                .any(|cmd| cmd.kind == BeautyPrimitiveKindV17::VehicleProxy)
    }
}
