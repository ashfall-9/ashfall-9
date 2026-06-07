//! V19 bridge from game/window context to artifact-free BeautySceneV19.
//!
//! This file is intentionally conservative: it creates a small mixed city/nature/landfill
//! golden scene with world-anchored objects. It does not place objects relative to the
//! camera. The camera may be used later for visibility budgeting only.

use ashfall_rendering::beauty_v19::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautySceneBuildContextV19 {
    pub world_seed: u64,
    pub frame_index: u64,
    pub camera_xy_for_visibility_only: [f32; 2],
    pub include_city: bool,
    pub include_nature: bool,
    pub include_landfill: bool,
}

impl Default for BeautySceneBuildContextV19 {
    fn default() -> Self {
        Self {
            world_seed: 0xA5FA_1119,
            frame_index: 0,
            camera_xy_for_visibility_only: [0.0, 0.0],
            include_city: true,
            include_nature: true,
            include_landfill: true,
        }
    }
}

pub fn build_beauty_scene_v19(ctx: BeautySceneBuildContextV19) -> BeautySceneV19 {
    let mut scene = BeautySceneV19::empty(ctx.world_seed);
    scene.environment = NaturalEnvironmentV19::overcast_city_nature_landfill();
    scene.frame_budget = FrameBudgetConfigV19::smooth_60hz();
    scene.material_recipes = default_materials_v19();

    if ctx.include_city {
        scene.cells.push(city_cell_v19(10));
    }
    if ctx.include_nature {
        scene.cells.push(nature_cell_v19(20));
    }
    if ctx.include_landfill {
        scene.cells.push(landfill_cell_v19(30));
    }
    scene
}

fn default_materials_v19() -> Vec<SurfaceTextureRecipeV19> {
    vec![
        SurfaceTextureRecipeV19::wet_asphalt(
            BeautySurfaceIdV19(10_001),
            BeautyMaterialIdV19(0xA5FA_1119),
        ),
        SurfaceTextureRecipeV19::new(
            BeautySurfaceIdV19(10_002),
            BeautyMaterialIdV19(0xC0A1_C0A1),
            MaterialClassV19::DirtyConcrete,
            1.2,
        )
        .with_surface_detail(0.76, 0.38, 0.74, 0.42)
        .with_weathering(0.72, 0.30, 0.32, 0.04)
        .with_channels(&[
            TextureChannelV19::BaseColor,
            TextureChannelV19::Normal,
            TextureChannelV19::Height,
            TextureChannelV19::Roughness,
            TextureChannelV19::AmbientOcclusion,
            TextureChannelV19::Dirt,
            TextureChannelV19::Wetness,
            TextureChannelV19::CrackChip,
        ]),
        SurfaceTextureRecipeV19::soil_mud(
            BeautySurfaceIdV19(20_001),
            BeautyMaterialIdV19(0x5011_0019),
        ),
        SurfaceTextureRecipeV19::stone(
            BeautySurfaceIdV19(20_002),
            BeautyMaterialIdV19(0x5700_E019),
        ),
        SurfaceTextureRecipeV19::plant_leaf(
            BeautySurfaceIdV19(20_003),
            BeautyMaterialIdV19(0x71A9_0019),
        ),
        SurfaceTextureRecipeV19::landfill_plastic(
            BeautySurfaceIdV19(30_001),
            BeautyMaterialIdV19(0x1A9D_F111),
        ),
        SurfaceTextureRecipeV19::car_paint(
            BeautySurfaceIdV19(10_004),
            BeautyMaterialIdV19(0xCA9_B0D7),
        ),
    ]
}

fn city_cell_v19(cell_id: u64) -> BeautyCellPackageV19 {
    let mut cell = BeautyCellPackageV19::new(
        cell_id,
        WorldBiomeV19::City,
        BoundsV19 {
            min: Vec3V19::new(-16.0, -12.0, -1.0),
            max: Vec3V19::new(16.0, 12.0, 8.0),
        },
    );
    cell.roads.push(RoadPatchV19 {
        surface_id: BeautySurfaceIdV19(10_001),
        material_id: BeautyMaterialIdV19(0xA5FA_1119),
        control_points: vec![
            Vec3V19::new(-15.0, -2.0, 0.0),
            Vec3V19::new(0.0, -1.2, 0.0),
            Vec3V19::new(15.0, -1.8, 0.0),
        ],
        width_meters: 5.8,
        camber_0_to_1: 0.22,
        crack_density_0_to_1: 0.36,
    });
    cell.curbs.push(CurbSegmentV19 {
        surface_id: BeautySurfaceIdV19(10_002),
        material_id: BeautyMaterialIdV19(0xC0A1_C0A1),
        start: Vec3V19::new(-15.0, 1.4, 0.05),
        end: Vec3V19::new(15.0, 1.1, 0.05),
        radius_meters: 0.14,
        chip_density_0_to_1: 0.44,
    });
    cell.facades.push(FacadeModuleV19 {
        surface_id: BeautySurfaceIdV19(10_002),
        material_id: BeautyMaterialIdV19(0xC0A1_C0A1),
        origin: Vec3V19::new(4.0, 5.2, 0.0),
        width_meters: 9.0,
        height_meters: 5.2,
        depth_meters: 0.42,
        bevel_radius_meters: 0.08,
        grime_0_to_1: 0.68,
    });
    cell.water_films.push(GroundedWaterFilmV19 {
        surface_id: BeautySurfaceIdV19(10_010),
        receiver_surface_id: BeautySurfaceIdV19(10_001),
        center: Vec3V19::new(-2.0, -1.4, 0.012),
        normal: Vec3V19::new(0.0, 0.0, 1.0),
        radius_meters: 1.1,
        depth_meters: 0.018,
        edge_irregularity_0_to_1: 0.74,
    });
    cell.humans.push(HumanProxyV19 {
        entity_id: 1001,
        world_position: Vec3V19::new(-4.0, 1.8, 0.0),
        height_meters: 1.74,
        shoulder_width_meters: 0.46,
        has_head: true,
        has_torso: true,
        has_limbs: true,
        has_clothing_material: true,
        has_skin_material: true,
        camera_relative: false,
    });
    cell.vehicles.push(VehicleProxyV19 {
        entity_id: 2001,
        world_position: Vec3V19::new(5.2, -2.4, 0.0),
        length_meters: 4.4,
        width_meters: 1.86,
        height_meters: 1.42,
        has_wheels_or_hover_equivalent: true,
        has_cabin: true,
        has_glass: true,
        has_lights: true,
        has_panel_seams: true,
        box_placeholder: false,
    });
    cell
}

fn nature_cell_v19(cell_id: u64) -> BeautyCellPackageV19 {
    let mut cell = BeautyCellPackageV19::new(
        cell_id,
        WorldBiomeV19::NatureReserve,
        BoundsV19 {
            min: Vec3V19::new(-18.0, 4.0, -1.0),
            max: Vec3V19::new(4.0, 22.0, 3.0),
        },
    );
    cell.terrain.push(TerrainPatchV19 {
        surface_id: BeautySurfaceIdV19(20_001),
        material_id: BeautyMaterialIdV19(0x5011_0019),
        center: Vec3V19::new(-8.0, 9.0, 0.0),
        size_meters: [18.0, 12.0],
        unevenness_0_to_1: 0.74,
        moisture_0_to_1: 0.48,
    });
    for i in 0..18 {
        let x = -15.0 + (i as f32 * 1.7) % 18.0;
        let y = 5.5 + (i as f32 * 2.23) % 12.0;
        cell.plants.push(PlantInstanceV19 {
            surface_id: BeautySurfaceIdV19(20_003),
            material_id: BeautyMaterialIdV19(0x71A9_0019),
            root_position: Vec3V19::new(x, y, 0.02),
            height_meters: 0.18 + (i % 5) as f32 * 0.10,
            bend_0_to_1: 0.25 + (i % 7) as f32 * 0.07,
            leaf_density_0_to_1: 0.42 + (i % 4) as f32 * 0.11,
        });
    }
    for i in 0..10 {
        cell.stones.push(StoneInstanceV19 {
            surface_id: BeautySurfaceIdV19(20_002),
            material_id: BeautyMaterialIdV19(0x5700_E019),
            center: Vec3V19::new(-14.0 + i as f32 * 1.8, 8.0 + (i % 3) as f32 * 1.4, 0.05),
            radius_meters: 0.12 + (i % 4) as f32 * 0.045,
            angularity_0_to_1: 0.55,
            chip_density_0_to_1: 0.34,
        });
    }
    cell.water_films.push(GroundedWaterFilmV19 {
        surface_id: BeautySurfaceIdV19(20_010),
        receiver_surface_id: BeautySurfaceIdV19(20_001),
        center: Vec3V19::new(-9.0, 12.4, 0.01),
        normal: Vec3V19::new(0.0, 0.0, 1.0),
        radius_meters: 0.84,
        depth_meters: 0.022,
        edge_irregularity_0_to_1: 0.88,
    });
    cell
}

fn landfill_cell_v19(cell_id: u64) -> BeautyCellPackageV19 {
    let mut cell = BeautyCellPackageV19::new(
        cell_id,
        WorldBiomeV19::Landfill,
        BoundsV19 {
            min: Vec3V19::new(6.0, 4.0, -1.0),
            max: Vec3V19::new(22.0, 22.0, 5.0),
        },
    );
    cell.terrain.push(TerrainPatchV19 {
        surface_id: BeautySurfaceIdV19(20_001),
        material_id: BeautyMaterialIdV19(0x5011_0019),
        center: Vec3V19::new(14.0, 12.0, 0.0),
        size_meters: [14.0, 14.0],
        unevenness_0_to_1: 0.92,
        moisture_0_to_1: 0.62,
    });
    for i in 0..16 {
        cell.landfill_props.push(LandfillPropV19 {
            surface_id: BeautySurfaceIdV19(30_001),
            material_id: BeautyMaterialIdV19(0x1A9D_F111),
            center: Vec3V19::new(
                8.0 + (i as f32 * 1.1) % 10.0,
                7.0 + (i as f32 * 1.7) % 11.0,
                0.08,
            ),
            size_meters: [
                0.35 + (i % 3) as f32 * 0.18,
                0.22 + (i % 5) as f32 * 0.12,
                0.12 + (i % 4) as f32 * 0.08,
            ],
            deformation_0_to_1: 0.64,
            dirt_0_to_1: 0.82,
            rust_or_stain_0_to_1: 0.48,
        });
    }
    cell.water_films.push(GroundedWaterFilmV19 {
        surface_id: BeautySurfaceIdV19(30_010),
        receiver_surface_id: BeautySurfaceIdV19(20_001),
        center: Vec3V19::new(12.0, 14.2, 0.01),
        normal: Vec3V19::new(0.0, 0.0, 1.0),
        radius_meters: 0.95,
        depth_meters: 0.030,
        edge_irregularity_0_to_1: 0.90,
    });
    cell
}
