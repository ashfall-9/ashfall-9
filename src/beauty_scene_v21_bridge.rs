//! V21 bridge from game/window context to a strict, artifact-free BeautySceneV21.
//!
//! This bridge intentionally builds a city + nature + landfill golden scene.
//! It does not place objects relative to the camera. The camera may be used
//! later for visibility budgeting only.

use ashfall_rendering::beauty_v21::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautySceneBuildContextV21 {
    pub frame_index: u64,
    pub camera_xy: [f32; 2],
    pub use_moonlit_lighting: bool,
    pub measured_frame_ms: f32,
}

impl Default for BeautySceneBuildContextV21 {
    fn default() -> Self {
        Self {
            frame_index: 0,
            camera_xy: [0.0, 0.0],
            use_moonlit_lighting: false,
            measured_frame_ms: 0.0,
        }
    }
}

pub fn build_beauty_scene_v21(ctx: BeautySceneBuildContextV21) -> BeautySceneV21 {
    let mut scene = BeautySceneV21::empty(21_000 + ctx.frame_index);
    scene.environment = if ctx.use_moonlit_lighting {
        NaturalEnvironmentV21::moonlit_overcast()
    } else {
        NaturalEnvironmentV21::city_nature_landfill_day()
    };
    if ctx.measured_frame_ms > scene.frame_budget.target_frame_ms * 1.25 {
        scene.frame_budget = FrameBudgetConfigV21::emergency_smooth();
    }

    scene.material_recipes = material_recipes_v21();
    scene.cells.push(city_cell_v21());
    scene.cells.push(nature_cell_v21());
    scene.cells.push(landfill_cell_v21());
    scene
}

fn material_recipes_v21() -> Vec<SurfaceTextureRecipeV21> {
    vec![
        SurfaceTextureRecipeV21::wet_asphalt(BeautySurfaceIdV21(10_001), BeautyMaterialIdV21(1)),
        SurfaceTextureRecipeV21::dirty_concrete(BeautySurfaceIdV21(10_002), BeautyMaterialIdV21(2)),
        SurfaceTextureRecipeV21::soil_mud(BeautySurfaceIdV21(20_001), BeautyMaterialIdV21(3)),
        SurfaceTextureRecipeV21::stone(BeautySurfaceIdV21(20_002), BeautyMaterialIdV21(4)),
        SurfaceTextureRecipeV21::plant_leaf(BeautySurfaceIdV21(20_003), BeautyMaterialIdV21(5)),
        SurfaceTextureRecipeV21::landfill_plastic(
            BeautySurfaceIdV21(30_001),
            BeautyMaterialIdV21(6),
        ),
        SurfaceTextureRecipeV21::human_skin(BeautySurfaceIdV21(40_001), BeautyMaterialIdV21(7)),
        SurfaceTextureRecipeV21::clothing_fabric(
            BeautySurfaceIdV21(40_002),
            BeautyMaterialIdV21(8),
        ),
        SurfaceTextureRecipeV21::car_paint(BeautySurfaceIdV21(50_001), BeautyMaterialIdV21(9)),
    ]
}

fn city_cell_v21() -> BeautyCellPackageV21 {
    let mut cell = BeautyCellPackageV21::empty(
        21_001,
        WorldBiomeV21::City,
        Bounds3V21::new(
            Vec3V21::new(-30.0, -18.0, 0.0),
            Vec3V21::new(12.0, 18.0, 10.0),
        ),
    );
    cell.roads.push(RoadPatchV21 {
        id: 1,
        centerline: vec![
            Vec3V21::new(-28.0, -3.0, 0.0),
            Vec3V21::new(-8.0, -1.5, 0.02),
            Vec3V21::new(10.0, 0.5, 0.0),
        ],
        width_meters: 6.8,
        unevenness_0_to_1: 0.22,
        camber_0_to_1: 0.16,
        surface: BeautySurfaceIdV21(10_001),
        material: BeautyMaterialIdV21(1),
    });
    for i in 0..6 {
        let x = -26.0 + i as f32 * 6.0;
        cell.curbs.push(CurbSegmentV21 {
            id: 100 + i,
            start: Vec3V21::new(x, -6.7, 0.0),
            end: Vec3V21::new(x + 5.4, -6.4, 0.0),
            height_meters: 0.16,
            bevel_radius_meters: 0.025,
            chip_density_0_to_1: 0.28,
            surface: BeautySurfaceIdV21(10_002),
            material: BeautyMaterialIdV21(2),
        });
    }
    cell.facades.push(FacadeModuleV21 {
        id: 200,
        bounds: Bounds3V21::new(Vec3V21::new(-25.0, 6.0, 0.0), Vec3V21::new(-8.0, 8.8, 7.5)),
        inset_depth_meters: 0.16,
        bevel_radius_meters: 0.018,
        window_count: 9,
        door_count: 1,
        pipe_count: 4,
        dirt_0_to_1: 0.72,
        surface: BeautySurfaceIdV21(10_002),
        material: BeautyMaterialIdV21(2),
    });
    cell.curves.push(CurveObjectV21 {
        id: 300,
        kind: CurveObjectKindV21::Cable,
        points: vec![
            Vec3V21::new(-24.0, 7.9, 5.8),
            Vec3V21::new(-16.0, 8.1, 5.5),
            Vec3V21::new(-9.0, 7.6, 5.9),
        ],
        radius_meters: 0.018,
        sag_0_to_1: 0.30,
        surface: BeautySurfaceIdV21(10_002),
        material: BeautyMaterialIdV21(2),
    });
    cell.water_films.push(GroundedWaterFilmV21 {
        id: 400,
        receiver_surface: BeautySurfaceIdV21(10_001),
        polygon: vec![
            Vec3V21::new(-12.0, -2.8, 0.015),
            Vec3V21::new(-8.2, -2.5, 0.015),
            Vec3V21::new(-7.6, -0.9, 0.015),
            Vec3V21::new(-11.4, -0.7, 0.015),
        ],
        normal: Vec3V21::new(0.0, 0.0, 1.0),
        depth_meters: 0.012,
        edge_softness_0_to_1: 0.65,
        roughness_0_to_1: 0.08,
    });
    cell.humans.push(HumanProxyV21::city_pedestrian(
        1_000,
        Vec3V21::new(-16.0, -5.3, 0.0),
    ));
    cell.vehicles.push(VehicleProxyV21::compact_car(
        2_000,
        Vec3V21::new(-7.0, -2.2, 0.0),
    ));
    cell.dirty = false;
    cell
}

fn nature_cell_v21() -> BeautyCellPackageV21 {
    let mut cell = BeautyCellPackageV21::empty(
        21_002,
        WorldBiomeV21::NatureReserve,
        Bounds3V21::new(
            Vec3V21::new(-8.0, -20.0, -0.4),
            Vec3V21::new(28.0, 18.0, 4.0),
        ),
    );
    cell.terrain.push(TerrainPatchV21 {
        id: 1,
        bounds: cell.bounds,
        height_variation_meters: 0.42,
        soil_surface: BeautySurfaceIdV21(20_001),
        soil_material: BeautyMaterialIdV21(3),
        stone_density_0_to_1: 0.55,
        plant_density_0_to_1: 0.76,
    });
    for i in 0..32 {
        let x = -4.0 + (i % 8) as f32 * 3.7;
        let y = -16.0 + (i / 8) as f32 * 6.8;
        cell.plants.push(PlantInstanceV21 {
            id: 500 + i as u64,
            position: Vec3V21::new(x, y, 0.02),
            height_meters: 0.18 + (i % 5) as f32 * 0.08,
            radius_meters: 0.06 + (i % 3) as f32 * 0.03,
            leaf_surface: BeautySurfaceIdV21(20_003),
            leaf_material: BeautyMaterialIdV21(5),
            wind_response_0_to_1: 0.35,
            clump_seed: 9_000 + i as u64,
        });
    }
    for i in 0..16 {
        cell.stones.push(StoneInstanceV21 {
            id: 700 + i as u64,
            position: Vec3V21::new(
                -6.0 + (i % 8) as f32 * 4.0,
                -11.0 + (i / 8) as f32 * 10.0,
                0.02,
            ),
            radius_meters: 0.05 + (i % 4) as f32 * 0.025,
            irregularity_0_to_1: 0.62,
            surface: BeautySurfaceIdV21(20_002),
            material: BeautyMaterialIdV21(4),
        });
    }
    cell.humans.push(HumanProxyV21::city_pedestrian(
        1_001,
        Vec3V21::new(3.5, -5.0, 0.0),
    ));
    cell.dirty = false;
    cell
}

fn landfill_cell_v21() -> BeautyCellPackageV21 {
    let mut cell = BeautyCellPackageV21::empty(
        21_003,
        WorldBiomeV21::Landfill,
        Bounds3V21::new(
            Vec3V21::new(18.0, -18.0, -0.2),
            Vec3V21::new(46.0, 18.0, 5.5),
        ),
    );
    cell.terrain.push(TerrainPatchV21 {
        id: 1,
        bounds: cell.bounds,
        height_variation_meters: 0.65,
        soil_surface: BeautySurfaceIdV21(20_001),
        soil_material: BeautyMaterialIdV21(3),
        stone_density_0_to_1: 0.42,
        plant_density_0_to_1: 0.18,
    });
    for i in 0..22 {
        let kind = match i % 5 {
            0 => LandfillPropKindV21::PlasticSheet,
            1 => LandfillPropKindV21::Cardboard,
            2 => LandfillPropKindV21::RustedMetalPanel,
            3 => LandfillPropKindV21::FabricBundle,
            _ => LandfillPropKindV21::Tire,
        };
        let x = 20.0 + (i % 7) as f32 * 3.6;
        let y = -14.0 + (i / 7) as f32 * 7.8;
        cell.landfill_props.push(LandfillPropV21 {
            id: 800 + i as u64,
            kind,
            position: Vec3V21::new(x, y, 0.05),
            bounds: Bounds3V21::new(
                Vec3V21::new(x - 0.45, y - 0.28, 0.0),
                Vec3V21::new(x + 0.45, y + 0.28, 0.35),
            ),
            deformation_0_to_1: 0.48,
            dirt_0_to_1: 0.82,
            surface: BeautySurfaceIdV21(30_001),
            material: BeautyMaterialIdV21(6),
        });
    }
    cell.water_films.push(GroundedWaterFilmV21 {
        id: 900,
        receiver_surface: BeautySurfaceIdV21(20_001),
        polygon: vec![
            Vec3V21::new(26.0, -8.0, 0.02),
            Vec3V21::new(31.0, -7.6, 0.02),
            Vec3V21::new(32.0, -4.0, 0.02),
            Vec3V21::new(25.0, -4.3, 0.02),
        ],
        normal: Vec3V21::new(0.0, 0.0, 1.0),
        depth_meters: 0.025,
        edge_softness_0_to_1: 0.72,
        roughness_0_to_1: 0.18,
    });
    cell.humans.push(HumanProxyV21::city_pedestrian(
        1_002,
        Vec3V21::new(24.0, 8.0, 0.0),
    ));
    cell.vehicles.push(VehicleProxyV21::landfill_loader(
        2_001,
        Vec3V21::new(36.0, -3.0, 0.0),
    ));
    cell.dirty = false;
    cell
}
