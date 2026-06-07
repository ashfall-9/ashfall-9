//! V20 bridge from game/window context to a strict, artifact-free BeautySceneV20.
//!
//! This bridge intentionally builds a small city + nature + landfill golden scene.
//! It does not place objects relative to the camera. The camera may be used later
//! for visibility budgeting only.

use ashfall_rendering::beauty_v20::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeautySceneBuildContextV20 {
    pub world_seed: u64,
    pub frame_index: u64,
    pub camera_xy_for_visibility_only: [f32; 2],
    pub include_city: bool,
    pub include_nature: bool,
    pub include_landfill: bool,
}

impl Default for BeautySceneBuildContextV20 {
    fn default() -> Self {
        Self {
            world_seed: 0xA5FA_2020,
            frame_index: 0,
            camera_xy_for_visibility_only: [0.0, 0.0],
            include_city: true,
            include_nature: true,
            include_landfill: true,
        }
    }
}

pub fn build_beauty_scene_v20(ctx: BeautySceneBuildContextV20) -> BeautySceneV20 {
    let mut scene = BeautySceneV20::empty(ctx.world_seed);
    scene.environment = NaturalEnvironmentV20::city_nature_landfill_day();
    scene.frame_budget = FrameBudgetConfigV20::smooth_60hz();
    scene.quarantine = BeautyModeQuarantineV20::strict_beauty();
    scene.artifact_counters = VisualArtifactCountersV20::default();
    scene.material_recipes = default_materials_v20();

    if ctx.include_city {
        scene.cells.push(city_cell_v20(10));
    }
    if ctx.include_nature {
        scene.cells.push(nature_cell_v20(20));
    }
    if ctx.include_landfill {
        scene.cells.push(landfill_cell_v20(30));
    }

    scene
}

pub fn default_materials_v20() -> Vec<SurfaceTextureRecipeV20> {
    vec![
        SurfaceTextureRecipeV20::wet_asphalt(
            BeautySurfaceIdV20(10_001),
            BeautyMaterialIdV20(0xA5FA_2020),
        ),
        SurfaceTextureRecipeV20::dirty_concrete(
            BeautySurfaceIdV20(10_002),
            BeautyMaterialIdV20(0xC0A1_2020),
        ),
        SurfaceTextureRecipeV20::rusted_metal(
            BeautySurfaceIdV20(10_003),
            BeautyMaterialIdV20(0xA9ED_2020),
        ),
        SurfaceTextureRecipeV20::glass(
            BeautySurfaceIdV20(10_004),
            BeautyMaterialIdV20(0x61A5_2020),
        ),
        SurfaceTextureRecipeV20::soil_mud(
            BeautySurfaceIdV20(20_001),
            BeautyMaterialIdV20(0x5011_2020),
        ),
        SurfaceTextureRecipeV20::stone(
            BeautySurfaceIdV20(20_002),
            BeautyMaterialIdV20(0x5700_2020),
        ),
        SurfaceTextureRecipeV20::plant_leaf(
            BeautySurfaceIdV20(20_003),
            BeautyMaterialIdV20(0x71A9_2020),
        ),
        SurfaceTextureRecipeV20::landfill_plastic(
            BeautySurfaceIdV20(30_001),
            BeautyMaterialIdV20(0x1A9D_2020),
        ),
        SurfaceTextureRecipeV20::rusted_metal(
            BeautySurfaceIdV20(30_002),
            BeautyMaterialIdV20(0xB057_2020),
        ),
        SurfaceTextureRecipeV20::landfill_fabric(
            BeautySurfaceIdV20(30_003),
            BeautyMaterialIdV20(0xFAB1_2020),
        ),
        SurfaceTextureRecipeV20::cardboard_paper(
            BeautySurfaceIdV20(30_004),
            BeautyMaterialIdV20(0xCA2D_2020),
        ),
        SurfaceTextureRecipeV20::human_skin(
            BeautySurfaceIdV20(40_001),
            BeautyMaterialIdV20(0x5A1E_2020),
        ),
        SurfaceTextureRecipeV20::clothing_fabric(
            BeautySurfaceIdV20(40_002),
            BeautyMaterialIdV20(0xC107_2020),
        ),
        SurfaceTextureRecipeV20::hair(BeautySurfaceIdV20(40_003), BeautyMaterialIdV20(0x0A17_2020)),
        SurfaceTextureRecipeV20::rubber(
            BeautySurfaceIdV20(40_004),
            BeautyMaterialIdV20(0x500E_2020),
        ),
        SurfaceTextureRecipeV20::car_paint(
            BeautySurfaceIdV20(50_001),
            BeautyMaterialIdV20(0xCA9_2020),
        ),
        SurfaceTextureRecipeV20::glass(
            BeautySurfaceIdV20(50_002),
            BeautyMaterialIdV20(0x61A5_2020),
        ),
        SurfaceTextureRecipeV20::rubber(
            BeautySurfaceIdV20(50_003),
            BeautyMaterialIdV20(0x700E_2020),
        ),
        SurfaceTextureRecipeV20::rusted_metal(
            BeautySurfaceIdV20(50_004),
            BeautyMaterialIdV20(0xD127_2020),
        ),
    ]
}

fn city_cell_v20(cell_id: u64) -> BeautyCellPackageV20 {
    let mut cell = BeautyCellPackageV20::new(
        cell_id,
        WorldBiomeV20::City,
        BoundsV20 {
            min: Vec3V20::new(-18.0, -14.0, -1.0),
            max: Vec3V20::new(18.0, 8.0, 9.0),
        },
    );

    cell.roads.push(RoadPatchV20 {
        surface_id: BeautySurfaceIdV20(10_001),
        material_id: BeautyMaterialIdV20(0xA5FA_2020),
        control_points: vec![
            Vec3V20::new(-17.0, -3.0, 0.0),
            Vec3V20::new(0.0, -2.1, 0.0),
            Vec3V20::new(17.0, -2.8, 0.0),
        ],
        width_meters: 6.2,
        camber_0_to_1: 0.22,
        unevenness_0_to_1: 0.38,
        crack_density_0_to_1: 0.42,
    });

    cell.curbs.push(CurbSegmentV20 {
        surface_id: BeautySurfaceIdV20(10_002),
        material_id: BeautyMaterialIdV20(0xC0A1_2020),
        start: Vec3V20::new(-17.0, 0.4, 0.05),
        end: Vec3V20::new(17.0, 0.1, 0.05),
        radius_meters: 0.16,
        chip_density_0_to_1: 0.48,
    });
    cell.curbs.push(CurbSegmentV20 {
        surface_id: BeautySurfaceIdV20(10_002),
        material_id: BeautyMaterialIdV20(0xC0A1_2020),
        start: Vec3V20::new(-17.0, -6.2, 0.05),
        end: Vec3V20::new(17.0, -6.0, 0.05),
        radius_meters: 0.14,
        chip_density_0_to_1: 0.36,
    });

    cell.facades.push(FacadeModuleV20 {
        surface_id: BeautySurfaceIdV20(10_002),
        material_id: BeautyMaterialIdV20(0xC0A1_2020),
        origin: Vec3V20::new(3.5, 3.6, 0.0),
        width_meters: 10.0,
        height_meters: 6.2,
        depth_meters: 0.55,
        bevel_radius_meters: 0.10,
        inset_window_count: 8,
        grime_0_to_1: 0.70,
    });
    cell.facades.push(FacadeModuleV20 {
        surface_id: BeautySurfaceIdV20(10_002),
        material_id: BeautyMaterialIdV20(0xC0A1_2020),
        origin: Vec3V20::new(-10.0, 2.8, 0.0),
        width_meters: 7.0,
        height_meters: 4.6,
        depth_meters: 0.44,
        bevel_radius_meters: 0.08,
        inset_window_count: 4,
        grime_0_to_1: 0.62,
    });
    cell.facades.push(FacadeModuleV20 {
        surface_id: BeautySurfaceIdV20(10_002),
        material_id: BeautyMaterialIdV20(0xC0A1_2020),
        origin: Vec3V20::new(-17.0, -10.6, 0.0),
        width_meters: 8.4,
        height_meters: 3.8,
        depth_meters: 0.40,
        bevel_radius_meters: 0.07,
        inset_window_count: 5,
        grime_0_to_1: 0.78,
    });
    cell.facades.push(FacadeModuleV20 {
        surface_id: BeautySurfaceIdV20(10_002),
        material_id: BeautyMaterialIdV20(0xC0A1_2020),
        origin: Vec3V20::new(9.8, -10.0, 0.0),
        width_meters: 6.0,
        height_meters: 3.2,
        depth_meters: 0.36,
        bevel_radius_meters: 0.06,
        inset_window_count: 3,
        grime_0_to_1: 0.66,
    });

    cell.curves.push(CurveObjectV20 {
        surface_id: BeautySurfaceIdV20(10_003),
        material_id: BeautyMaterialIdV20(0xA9ED_2020),
        kind: CurveObjectKindV20::Pipe,
        points: vec![
            Vec3V20::new(-8.0, 3.1, 1.1),
            Vec3V20::new(-1.0, 3.3, 1.3),
            Vec3V20::new(6.0, 3.0, 1.1),
        ],
        radius_meters: 0.055,
        sag_0_to_1: 0.08,
        dirt_0_to_1: 0.72,
    });
    cell.curves.push(CurveObjectV20 {
        surface_id: BeautySurfaceIdV20(10_003),
        material_id: BeautyMaterialIdV20(0xA9ED_2020),
        kind: CurveObjectKindV20::Cable,
        points: vec![
            Vec3V20::new(-12.0, 2.9, 3.8),
            Vec3V20::new(-2.0, 3.2, 3.2),
            Vec3V20::new(9.0, 2.7, 3.6),
        ],
        radius_meters: 0.018,
        sag_0_to_1: 0.42,
        dirt_0_to_1: 0.44,
    });
    cell.curves.push(CurveObjectV20 {
        surface_id: BeautySurfaceIdV20(10_003),
        material_id: BeautyMaterialIdV20(0xA9ED_2020),
        kind: CurveObjectKindV20::Rail,
        points: vec![
            Vec3V20::new(-15.0, -6.7, 0.24),
            Vec3V20::new(-7.0, -6.5, 0.26),
            Vec3V20::new(3.0, -6.7, 0.25),
            Vec3V20::new(14.5, -6.4, 0.27),
        ],
        radius_meters: 0.030,
        sag_0_to_1: 0.02,
        dirt_0_to_1: 0.58,
    });
    cell.curves.push(CurveObjectV20 {
        surface_id: BeautySurfaceIdV20(10_003),
        material_id: BeautyMaterialIdV20(0xA9ED_2020),
        kind: CurveObjectKindV20::Hose,
        points: vec![
            Vec3V20::new(-6.8, -7.8, 0.05),
            Vec3V20::new(-5.6, -8.3, 0.04),
            Vec3V20::new(-4.1, -7.6, 0.05),
            Vec3V20::new(-2.7, -8.0, 0.04),
        ],
        radius_meters: 0.035,
        sag_0_to_1: 0.12,
        dirt_0_to_1: 0.82,
    });

    for i in 0..12 {
        let (surface_id, material_id) = match i % 4 {
            0 => (BeautySurfaceIdV20(30_001), BeautyMaterialIdV20(0x1A9D_2020)),
            1 => (BeautySurfaceIdV20(30_003), BeautyMaterialIdV20(0xFAB1_2020)),
            2 => (BeautySurfaceIdV20(30_004), BeautyMaterialIdV20(0xCA2D_2020)),
            _ => (BeautySurfaceIdV20(30_002), BeautyMaterialIdV20(0xB057_2020)),
        };
        cell.landfill_props.push(LandfillPropV20 {
            surface_id,
            material_id,
            center: Vec3V20::new(-14.0 + i as f32 * 1.9, -7.6 + (i % 3) as f32 * 0.42, 0.08),
            size_meters: [
                0.28 + (i % 3) as f32 * 0.08,
                0.18 + (i % 5) as f32 * 0.05,
                0.08 + (i % 4) as f32 * 0.04,
            ],
            deformation_0_to_1: 0.58,
            dirt_0_to_1: 0.84,
            rust_or_stain_0_to_1: 0.38,
            sharp_edges_0_to_1: 0.22,
        });
    }

    cell.water_films.push(GroundedWaterFilmV20 {
        surface_id: BeautySurfaceIdV20(10_010),
        receiver_surface_id: BeautySurfaceIdV20(10_001),
        center: Vec3V20::new(-2.5, -2.6, 0.012),
        normal: Vec3V20::new(0.0, 0.0, 1.0),
        radius_meters: 1.15,
        depth_meters: 0.016,
        edge_irregularity_0_to_1: 0.78,
        reflection_quality_0_to_1: 0.62,
    });
    cell.water_films.push(GroundedWaterFilmV20 {
        surface_id: BeautySurfaceIdV20(10_011),
        receiver_surface_id: BeautySurfaceIdV20(10_001),
        center: Vec3V20::new(7.2, -3.8, 0.012),
        normal: Vec3V20::new(0.0, 0.0, 1.0),
        radius_meters: 0.74,
        depth_meters: 0.012,
        edge_irregularity_0_to_1: 0.82,
        reflection_quality_0_to_1: 0.48,
    });
    cell.water_films.push(GroundedWaterFilmV20 {
        surface_id: BeautySurfaceIdV20(10_012),
        receiver_surface_id: BeautySurfaceIdV20(10_001),
        center: Vec3V20::new(-11.4, -5.4, 0.012),
        normal: Vec3V20::new(0.0, 0.0, 1.0),
        radius_meters: 0.48,
        depth_meters: 0.010,
        edge_irregularity_0_to_1: 0.88,
        reflection_quality_0_to_1: 0.34,
    });

    cell.humans.push(pedestrian_v20(
        1001,
        Vec3V20::new(-4.0, 0.9, 0.0),
        -0.4,
        HumanPoseStateV20::Walking,
        1.74,
    ));
    cell.humans.push(pedestrian_v20(
        1002,
        Vec3V20::new(1.2, 0.7, 0.0),
        0.35,
        HumanPoseStateV20::Idle,
        1.68,
    ));
    cell.humans.push(pedestrian_v20(
        1004,
        Vec3V20::new(-12.3, -6.85, 0.0),
        0.05,
        HumanPoseStateV20::Crouched,
        1.80,
    ));
    cell.humans.push(pedestrian_v20(
        1005,
        Vec3V20::new(10.8, 0.35, 0.0),
        2.7,
        HumanPoseStateV20::Walking,
        1.86,
    ));
    cell.vehicles.push(VehicleProxyV20::parked_sedan(
        2001,
        Vec3V20::new(6.2, -4.2, 0.0),
    ));
    let mut van = VehicleProxyV20::parked_sedan(2003, Vec3V20::new(-8.2, -4.9, 0.0));
    van.kind = VehicleKindV20::Van;
    van.facing_yaw_radians = -0.04;
    van.length_meters = 4.92;
    van.width_meters = 2.05;
    van.height_meters = 1.94;
    cell.vehicles.push(van);
    let mut utility = VehicleProxyV20::parked_sedan(2004, Vec3V20::new(12.8, -3.8, 0.0));
    utility.kind = VehicleKindV20::UtilityTruck;
    utility.facing_yaw_radians = 0.18;
    utility.length_meters = 5.35;
    utility.width_meters = 2.16;
    utility.height_meters = 1.86;
    cell.vehicles.push(utility);

    cell
}

fn nature_cell_v20(cell_id: u64) -> BeautyCellPackageV20 {
    let mut cell = BeautyCellPackageV20::new(
        cell_id,
        WorldBiomeV20::NatureReserve,
        BoundsV20 {
            min: Vec3V20::new(-18.0, 4.0, -1.0),
            max: Vec3V20::new(6.0, 25.0, 4.0),
        },
    );

    cell.terrain.push(TerrainPatchV20 {
        surface_id: BeautySurfaceIdV20(20_001),
        material_id: BeautyMaterialIdV20(0x5011_2020),
        center: Vec3V20::new(-7.5, 13.0, 0.0),
        size_meters: [21.0, 16.0],
        unevenness_0_to_1: 0.80,
        moisture_0_to_1: 0.52,
        vegetation_coverage_0_to_1: 0.58,
    });

    for i in 0..72 {
        let x = -16.0 + (i as f32 * 1.43) % 21.0;
        let y = 5.4 + (i as f32 * 2.11) % 16.0;
        let tall_growth = i % 9 == 0 || i % 17 == 0;
        cell.plants.push(PlantInstanceV20 {
            surface_id: BeautySurfaceIdV20(20_003),
            material_id: BeautyMaterialIdV20(0x71A9_2020),
            root_position: Vec3V20::new(x, y, 0.02),
            height_meters: if tall_growth {
                0.76 + (i % 5) as f32 * 0.12
            } else {
                0.16 + (i % 7) as f32 * 0.08
            },
            radius_meters: if tall_growth {
                0.18 + (i % 4) as f32 * 0.04
            } else {
                0.10 + (i % 4) as f32 * 0.03
            },
            bend_0_to_1: 0.20 + (i % 6) as f32 * 0.08,
            leaf_density_0_to_1: if tall_growth {
                0.72 + (i % 3) as f32 * 0.06
            } else {
                0.40 + (i % 5) as f32 * 0.10
            },
            variation_seed: 0x7100_2020 ^ i as u64,
        });
    }

    for i in 0..28 {
        cell.stones.push(StoneInstanceV20 {
            surface_id: BeautySurfaceIdV20(20_002),
            material_id: BeautyMaterialIdV20(0x5700_2020),
            center: Vec3V20::new(
                -15.5 + (i as f32 * 1.35) % 20.0,
                6.2 + (i as f32 * 2.05) % 17.0,
                0.05,
            ),
            radius_meters: 0.10 + (i % 7) as f32 * 0.040,
            angularity_0_to_1: 0.42 + (i % 5) as f32 * 0.08,
            chip_density_0_to_1: 0.26 + (i % 4) as f32 * 0.08,
        });
    }
    for root in 0..6 {
        let x = -15.0 + root as f32 * 3.4;
        let y = 10.0 + (root % 3) as f32 * 3.0;
        cell.curves.push(CurveObjectV20 {
            surface_id: BeautySurfaceIdV20(20_003),
            material_id: BeautyMaterialIdV20(0x71A9_2020),
            kind: CurveObjectKindV20::Root,
            points: vec![
                Vec3V20::new(x, y, 0.04),
                Vec3V20::new(x + 0.9, y + 0.32, 0.05),
                Vec3V20::new(x + 1.8, y - 0.10, 0.04),
            ],
            radius_meters: 0.026 + (root % 3) as f32 * 0.006,
            sag_0_to_1: 0.04,
            dirt_0_to_1: 0.70,
        });
    }

    cell.water_films.push(GroundedWaterFilmV20 {
        surface_id: BeautySurfaceIdV20(20_010),
        receiver_surface_id: BeautySurfaceIdV20(20_001),
        center: Vec3V20::new(-8.0, 13.4, 0.010),
        normal: Vec3V20::new(0.0, 0.0, 1.0),
        radius_meters: 0.86,
        depth_meters: 0.020,
        edge_irregularity_0_to_1: 0.90,
        reflection_quality_0_to_1: 0.36,
    });
    cell.water_films.push(GroundedWaterFilmV20 {
        surface_id: BeautySurfaceIdV20(20_011),
        receiver_surface_id: BeautySurfaceIdV20(20_001),
        center: Vec3V20::new(-14.0, 18.6, 0.010),
        normal: Vec3V20::new(0.0, 0.0, 1.0),
        radius_meters: 0.62,
        depth_meters: 0.018,
        edge_irregularity_0_to_1: 0.94,
        reflection_quality_0_to_1: 0.28,
    });
    cell.humans.push(pedestrian_v20(
        1003,
        Vec3V20::new(-3.0, 8.2, 0.0),
        2.2,
        HumanPoseStateV20::Walking,
        1.70,
    ));
    cell.humans.push(pedestrian_v20(
        1006,
        Vec3V20::new(-11.2, 17.4, 0.0),
        -0.8,
        HumanPoseStateV20::Crouched,
        1.62,
    ));
    cell
}

fn landfill_cell_v20(cell_id: u64) -> BeautyCellPackageV20 {
    let mut cell = BeautyCellPackageV20::new(
        cell_id,
        WorldBiomeV20::Landfill,
        BoundsV20 {
            min: Vec3V20::new(6.0, 4.0, -1.0),
            max: Vec3V20::new(25.0, 25.0, 6.0),
        },
    );

    cell.terrain.push(TerrainPatchV20 {
        surface_id: BeautySurfaceIdV20(20_001),
        material_id: BeautyMaterialIdV20(0x5011_2020),
        center: Vec3V20::new(15.0, 14.0, 0.0),
        size_meters: [16.0, 16.0],
        unevenness_0_to_1: 0.94,
        moisture_0_to_1: 0.66,
        vegetation_coverage_0_to_1: 0.12,
    });

    for i in 0..68 {
        let (surface_id, material_id) = match i % 5 {
            0 => (BeautySurfaceIdV20(30_001), BeautyMaterialIdV20(0x1A9D_2020)),
            1 => (BeautySurfaceIdV20(30_002), BeautyMaterialIdV20(0xB057_2020)),
            2 => (BeautySurfaceIdV20(30_003), BeautyMaterialIdV20(0xFAB1_2020)),
            3 => (BeautySurfaceIdV20(30_004), BeautyMaterialIdV20(0xCA2D_2020)),
            _ => (BeautySurfaceIdV20(50_003), BeautyMaterialIdV20(0x700E_2020)),
        };
        cell.landfill_props.push(LandfillPropV20 {
            surface_id,
            material_id,
            center: Vec3V20::new(
                8.0 + (i as f32 * 1.17) % 14.0,
                7.0 + (i as f32 * 1.83) % 14.0,
                0.08 + (i % 4) as f32 * 0.025,
            ),
            size_meters: [
                0.30 + (i % 4) as f32 * 0.14,
                0.18 + (i % 5) as f32 * 0.09,
                0.10 + (i % 6) as f32 * 0.05,
            ],
            deformation_0_to_1: 0.52 + (i % 5) as f32 * 0.08,
            dirt_0_to_1: 0.74 + (i % 4) as f32 * 0.06,
            rust_or_stain_0_to_1: if surface_id == BeautySurfaceIdV20(30_002) {
                0.72
            } else {
                0.34 + (i % 6) as f32 * 0.06
            },
            sharp_edges_0_to_1: if surface_id == BeautySurfaceIdV20(30_002) {
                0.48
            } else {
                0.18 + (i % 4) as f32 * 0.06
            },
        });
    }
    for cable in 0..5 {
        let x = 8.4 + cable as f32 * 2.8;
        let y = 6.6 + (cable % 2) as f32 * 8.0;
        cell.curves.push(CurveObjectV20 {
            surface_id: BeautySurfaceIdV20(10_003),
            material_id: BeautyMaterialIdV20(0xA9ED_2020),
            kind: if cable % 2 == 0 {
                CurveObjectKindV20::Hose
            } else {
                CurveObjectKindV20::Cable
            },
            points: vec![
                Vec3V20::new(x, y, 0.05),
                Vec3V20::new(x + 1.4, y + 0.8, 0.04),
                Vec3V20::new(x + 2.2, y + 0.2, 0.05),
            ],
            radius_meters: 0.024 + (cable % 3) as f32 * 0.006,
            sag_0_to_1: 0.18,
            dirt_0_to_1: 0.86,
        });
    }

    cell.water_films.push(GroundedWaterFilmV20 {
        surface_id: BeautySurfaceIdV20(30_010),
        receiver_surface_id: BeautySurfaceIdV20(20_001),
        center: Vec3V20::new(13.0, 15.0, 0.010),
        normal: Vec3V20::new(0.0, 0.0, 1.0),
        radius_meters: 1.05,
        depth_meters: 0.030,
        edge_irregularity_0_to_1: 0.92,
        reflection_quality_0_to_1: 0.26,
    });
    cell.water_films.push(GroundedWaterFilmV20 {
        surface_id: BeautySurfaceIdV20(30_011),
        receiver_surface_id: BeautySurfaceIdV20(20_001),
        center: Vec3V20::new(21.0, 18.2, 0.010),
        normal: Vec3V20::new(0.0, 0.0, 1.0),
        radius_meters: 0.72,
        depth_meters: 0.024,
        edge_irregularity_0_to_1: 0.88,
        reflection_quality_0_to_1: 0.20,
    });
    let mut machine = VehicleProxyV20::landfill_machine(2002, Vec3V20::new(18.8, 10.4, 0.0));
    machine.materials.body_surface = BeautySurfaceIdV20(30_002);
    machine.materials.body_material = BeautyMaterialIdV20(0xB057_2020);
    machine.materials.dirt_surface = BeautySurfaceIdV20(30_001);
    machine.materials.dirt_material = BeautyMaterialIdV20(0x1A9D_2020);
    cell.vehicles.push(machine);
    let mut loader = VehicleProxyV20::landfill_machine(2005, Vec3V20::new(11.0, 19.0, 0.0));
    loader.length_meters = 4.7;
    loader.width_meters = 2.15;
    loader.height_meters = 2.15;
    loader.facing_yaw_radians = -0.62;
    loader.materials.body_surface = BeautySurfaceIdV20(30_002);
    loader.materials.body_material = BeautyMaterialIdV20(0xB057_2020);
    cell.vehicles.push(loader);
    cell.humans.push(pedestrian_v20(
        1007,
        Vec3V20::new(9.5, 10.8, 0.0),
        0.9,
        HumanPoseStateV20::Walking,
        1.76,
    ));
    cell
}

fn pedestrian_v20(
    entity_id: u64,
    world_position: Vec3V20,
    facing_yaw_radians: f32,
    pose: HumanPoseStateV20,
    height_meters: f32,
) -> HumanProxyV20 {
    let mut human = HumanProxyV20::city_pedestrian(entity_id, world_position);
    human.facing_yaw_radians = facing_yaw_radians;
    human.pose = pose;
    human.proportions.height_meters = height_meters.clamp(1.45, 2.05);
    human.proportions.leg_length_meters =
        (human.proportions.height_meters * 0.50).clamp(0.62, 1.02);
    human.proportions.arm_length_meters =
        (human.proportions.height_meters * 0.41).clamp(0.50, 0.86);
    human
}
