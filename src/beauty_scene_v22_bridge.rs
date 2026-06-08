//! V22 bridge from game/window context to strict, artifact-free BeautySceneV22.
//!
//! This bridge intentionally builds a city + nature + landfill golden scene.
//! It does not place objects relative to the camera. The camera may be used
//! later for visibility budgeting only.

use ashfall_rendering::beauty_v22::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautySceneBuildContextV22 {
    pub frame_index: u64,
    pub camera_xy: [f32; 2],
    pub use_moonlit_lighting: bool,
    pub measured_frame_ms: f32,
    pub measured_cpu_scene_build_ms: f32,
    pub measured_gpu_scene_ms: f32,
    pub legacy_paths_active: LegacyBeautyPathFlagsV22,
}

impl Default for BeautySceneBuildContextV22 {
    fn default() -> Self {
        Self {
            frame_index: 0,
            camera_xy: [0.0, 0.0],
            use_moonlit_lighting: false,
            measured_frame_ms: 0.0,
            measured_cpu_scene_build_ms: 0.0,
            measured_gpu_scene_ms: 0.0,
            legacy_paths_active: LegacyBeautyPathFlagsV22::default(),
        }
    }
}

pub fn build_beauty_scene_v22(ctx: BeautySceneBuildContextV22) -> BeautySceneV22 {
    let mut scene = BeautySceneV22::empty(22_000 + ctx.frame_index);

    scene.environment = if ctx.use_moonlit_lighting {
        NaturalEnvironmentV22::moonlit_landfill_rain()
    } else {
        NaturalEnvironmentV22::city_nature_landfill_overcast_day()
    };

    scene.legacy_path_flags = ctx.legacy_paths_active;
    scene.artifact_counters = ctx.legacy_paths_active.to_counters();

    if ctx.measured_frame_ms > scene.frame_budget.target_frame_ms * 1.15
        || ctx.measured_cpu_scene_build_ms > scene.frame_budget.target_cpu_scene_build_ms * 1.15
        || ctx.measured_gpu_scene_ms > scene.frame_budget.target_gpu_scene_ms * 1.15
    {
        scene.frame_budget = FrameBudgetConfigV22::emergency_smooth();
    }

    scene.material_recipes = material_recipes_v22();
    scene.cells.push(city_cell_v22());
    scene.cells.push(nature_cell_v22());
    scene.cells.push(landfill_cell_v22());

    scene
}

fn material_recipes_v22() -> Vec<SurfaceTextureRecipeV22> {
    vec![
        SurfaceTextureRecipeV22::wet_asphalt(BeautySurfaceIdV22(10_001), BeautyMaterialIdV22(1)),
        SurfaceTextureRecipeV22::dirty_concrete(BeautySurfaceIdV22(10_002), BeautyMaterialIdV22(2)),
        SurfaceTextureRecipeV22::soil_mud(BeautySurfaceIdV22(20_001), BeautyMaterialIdV22(3)),
        SurfaceTextureRecipeV22::stone(BeautySurfaceIdV22(20_002), BeautyMaterialIdV22(4)),
        SurfaceTextureRecipeV22::plant_leaf(BeautySurfaceIdV22(20_003), BeautyMaterialIdV22(5)),
        SurfaceTextureRecipeV22::landfill_plastic(
            BeautySurfaceIdV22(30_001),
            BeautyMaterialIdV22(6),
        ),
        SurfaceTextureRecipeV22::human_skin(BeautySurfaceIdV22(40_001), BeautyMaterialIdV22(7)),
        SurfaceTextureRecipeV22::clothing_fabric(
            BeautySurfaceIdV22(40_002),
            BeautyMaterialIdV22(8),
        ),
        SurfaceTextureRecipeV22::car_paint(BeautySurfaceIdV22(50_001), BeautyMaterialIdV22(9)),
    ]
}

fn city_cell_v22() -> BeautyCellPackageV22 {
    let mut cell = BeautyCellPackageV22::empty(
        22_001,
        WorldBiomeV22::City,
        Bounds3V22::new(
            Vec3V22::new(-30.0, -18.0, 0.0),
            Vec3V22::new(12.0, 18.0, 10.0),
        ),
    );

    cell.roads.push(RoadPatchV22 {
        id: 1,
        centerline: vec![
            Vec3V22::new(-28.0, -3.0, 0.0),
            Vec3V22::new(-8.0, -1.5, 0.02),
            Vec3V22::new(10.0, 0.5, 0.0),
        ],
        width_meters: 6.8,
        unevenness_0_to_1: 0.24,
        camber_0_to_1: 0.16,
        mesh: VisualMeshRefV22::generated(1001, MeshSourceKindV22::GeneratedRoadStrip),
        surface: BeautySurfaceIdV22(10_001),
        material: BeautyMaterialIdV22(1),
    });

    for i in 0..8 {
        let x = -28.0 + i as f32 * 5.0;
        cell.curbs.push(CurbSegmentV22 {
            id: 100 + i,
            start: Vec3V22::new(x, -6.7, 0.0),
            end: Vec3V22::new(x + 4.6, -6.4, 0.0),
            height_meters: 0.16,
            bevel_radius_meters: 0.030,
            chip_density_0_to_1: 0.30,
            mesh: VisualMeshRefV22::generated(2100 + i, MeshSourceKindV22::GeneratedCurb),
            surface: BeautySurfaceIdV22(10_002),
            material: BeautyMaterialIdV22(2),
        });
    }

    cell.facades.push(FacadeModuleV22 {
        id: 200,
        bounds: Bounds3V22::new(Vec3V22::new(-25.0, 6.0, 0.0), Vec3V22::new(-8.0, 8.8, 7.5)),
        inset_depth_meters: 0.18,
        bevel_radius_meters: 0.022,
        window_count: 9,
        door_count: 1,
        pipe_count: 4,
        dirt_0_to_1: 0.74,
        mesh: VisualMeshRefV22::generated(3200, MeshSourceKindV22::GeneratedFacade),
        surface: BeautySurfaceIdV22(10_002),
        material: BeautyMaterialIdV22(2),
    });

    cell.curves.push(CurveObjectV22 {
        id: 300,
        kind: CurveObjectKindV22::Cable,
        points: vec![
            Vec3V22::new(-24.0, 7.9, 5.8),
            Vec3V22::new(-16.0, 8.1, 5.5),
            Vec3V22::new(-9.0, 7.6, 5.9),
        ],
        radius_meters: 0.018,
        sag_0_to_1: 0.30,
        mesh: VisualMeshRefV22::generated(4300, MeshSourceKindV22::GeneratedCurveTube),
        surface: BeautySurfaceIdV22(10_002),
        material: BeautyMaterialIdV22(2),
    });

    cell.water_films.push(GroundedWaterFilmV22 {
        id: 400,
        receiver_surface: BeautySurfaceIdV22(10_001),
        polygon: vec![
            Vec3V22::new(-12.0, -2.8, 0.015),
            Vec3V22::new(-8.2, -2.5, 0.015),
            Vec3V22::new(-7.6, -0.9, 0.015),
            Vec3V22::new(-11.4, -0.7, 0.015),
        ],
        normal: Vec3V22::new(0.0, 0.0, 1.0),
        depth_meters: 0.012,
        edge_softness_0_to_1: 0.65,
        roughness_0_to_1: 0.08,
        screen_space_or_billboard: false,
    });

    cell.humans
        .push(HumanProxyV22::world_anchored_coherent_pedestrian(
            1_000,
            Vec3V22::new(-16.0, -5.3, 0.0),
        ));
    cell.vehicles.push(VehicleProxyV22::compact_car(
        2_000,
        Vec3V22::new(-7.0, -2.2, 0.0),
    ));

    cell.dirty = false;
    cell
}

fn nature_cell_v22() -> BeautyCellPackageV22 {
    let mut cell = BeautyCellPackageV22::empty(
        22_002,
        WorldBiomeV22::NatureReserve,
        Bounds3V22::new(
            Vec3V22::new(-8.0, -20.0, -0.4),
            Vec3V22::new(28.0, 18.0, 4.0),
        ),
    );

    cell.terrain.push(TerrainPatchV22 {
        id: 1,
        bounds: cell.bounds,
        height_variation_meters: 0.48,
        mesh: VisualMeshRefV22::generated(5100, MeshSourceKindV22::GeneratedTerrainPatch),
        soil_surface: BeautySurfaceIdV22(20_001),
        soil_material: BeautyMaterialIdV22(3),
        stone_density_0_to_1: 0.55,
        plant_density_0_to_1: 0.76,
    });

    for i in 0..40 {
        let x = -4.0 + (i % 8) as f32 * 3.7;
        let y = -16.0 + (i / 8) as f32 * 5.4;
        cell.plants.push(PlantInstanceV22 {
            id: 500 + i as u64,
            position: Vec3V22::new(x, y, 0.02),
            height_meters: 0.18 + (i % 5) as f32 * 0.08,
            radius_meters: 0.06 + (i % 3) as f32 * 0.03,
            cluster_mesh: VisualMeshRefV22::generated(
                6000 + i as u64,
                MeshSourceKindV22::GeneratedPlantCluster,
            ),
            leaf_surface: BeautySurfaceIdV22(20_003),
            leaf_material: BeautyMaterialIdV22(5),
            wind_response_0_to_1: 0.35,
            clump_seed: 9_000 + i as u64,
        });
    }

    for i in 0..18 {
        cell.stones.push(StoneInstanceV22 {
            id: 700 + i as u64,
            position: Vec3V22::new(
                -6.0 + (i % 9) as f32 * 3.6,
                -11.0 + (i / 9) as f32 * 9.0,
                0.02,
            ),
            radius_meters: 0.05 + (i % 4) as f32 * 0.025,
            irregularity_0_to_1: 0.64,
            mesh: VisualMeshRefV22::generated(
                7000 + i as u64,
                MeshSourceKindV22::GeneratedStoneCluster,
            ),
            surface: BeautySurfaceIdV22(20_002),
            material: BeautyMaterialIdV22(4),
        });
    }

    cell.humans
        .push(HumanProxyV22::world_anchored_coherent_pedestrian(
            1_001,
            Vec3V22::new(3.5, -5.0, 0.0),
        ));

    cell.dirty = false;
    cell
}

fn landfill_cell_v22() -> BeautyCellPackageV22 {
    let mut cell = BeautyCellPackageV22::empty(
        22_003,
        WorldBiomeV22::Landfill,
        Bounds3V22::new(
            Vec3V22::new(18.0, -18.0, -0.2),
            Vec3V22::new(46.0, 18.0, 5.5),
        ),
    );

    cell.terrain.push(TerrainPatchV22 {
        id: 1,
        bounds: cell.bounds,
        height_variation_meters: 0.68,
        mesh: VisualMeshRefV22::generated(8100, MeshSourceKindV22::GeneratedTerrainPatch),
        soil_surface: BeautySurfaceIdV22(20_001),
        soil_material: BeautyMaterialIdV22(3),
        stone_density_0_to_1: 0.42,
        plant_density_0_to_1: 0.18,
    });

    for i in 0..24 {
        let kind = match i % 5 {
            0 => LandfillPropKindV22::PlasticSheet,
            1 => LandfillPropKindV22::Cardboard,
            2 => LandfillPropKindV22::RustedMetalPanel,
            3 => LandfillPropKindV22::FabricBundle,
            _ => LandfillPropKindV22::Tire,
        };
        let x = 20.0 + (i % 8) as f32 * 3.2;
        let y = -14.0 + (i / 8) as f32 * 7.0;
        cell.landfill_props.push(LandfillPropV22 {
            id: 800 + i as u64,
            kind,
            position: Vec3V22::new(x, y, 0.05),
            bounds: Bounds3V22::new(
                Vec3V22::new(x - 0.45, y - 0.28, 0.0),
                Vec3V22::new(x + 0.45, y + 0.28, 0.35),
            ),
            deformation_0_to_1: 0.50,
            dirt_0_to_1: 0.84,
            mesh: VisualMeshRefV22::generated(
                9000 + i as u64,
                MeshSourceKindV22::GeneratedLandfillProp,
            ),
            surface: BeautySurfaceIdV22(30_001),
            material: BeautyMaterialIdV22(6),
        });
    }

    cell.water_films.push(GroundedWaterFilmV22 {
        id: 900,
        receiver_surface: BeautySurfaceIdV22(20_001),
        polygon: vec![
            Vec3V22::new(26.0, -8.0, 0.02),
            Vec3V22::new(31.0, -7.6, 0.02),
            Vec3V22::new(32.0, -4.0, 0.02),
            Vec3V22::new(25.0, -4.3, 0.02),
        ],
        normal: Vec3V22::new(0.0, 0.0, 1.0),
        depth_meters: 0.025,
        edge_softness_0_to_1: 0.72,
        roughness_0_to_1: 0.18,
        screen_space_or_billboard: false,
    });

    cell.humans
        .push(HumanProxyV22::world_anchored_coherent_pedestrian(
            1_002,
            Vec3V22::new(24.0, 8.0, 0.0),
        ));
    cell.vehicles.push(VehicleProxyV22::landfill_loader(
        2_001,
        Vec3V22::new(36.0, -3.0, 0.0),
    ));

    cell.dirty = false;
    cell
}
