use ashfall_rendering::beauty_v17::*;

#[test]
fn v17_environment_has_sky_sun_or_moon_and_is_readable_without_neon() {
    let day = EnvironmentStateV17::rainy_overcast_day().with_neon_disabled();
    assert!(day.has_proper_sky());
    assert!(day.has_sun_or_moon());
    assert!(day.has_natural_light());
    assert!(day.readable_without_neon());

    let night = EnvironmentStateV17::rainy_moonlit_night().with_neon_disabled();
    assert!(night.has_proper_sky());
    assert!(night.has_sun_or_moon());
    assert!(night.has_natural_light());
    assert!(night.readable_without_neon());
}

#[test]
fn v17_rejects_debug_leaks_in_beauty_mode() {
    let mut scene = BeautySceneV17::new(0);
    scene.debug_leaks = BeautyDebugLeakFlagsV17 {
        chunk_bounds_visible: true,
        ..BeautyDebugLeakFlagsV17::default()
    };
    let report = scene.validate_for_visual_realism();
    assert!(!report.pass);
    assert!(
        report
            .failure_reasons
            .contains(&BeautyFailureReasonV17::DebugPrimitiveLeak)
    );
}

#[test]
fn v17_rejects_floating_puddles() {
    let puddle = GroundedPuddleV17 {
        object_id: BeautyObjectIdV17(1),
        receiver_surface_id: BeautySurfaceIdV17(0),
        center_world: [0.0, 0.0, 3.0],
        receiver_normal_world: [0.0, 0.0, 0.0],
        radius_meters: 1.0,
        max_depth_meters: 0.01,
        edge_softness_meters: 0.1,
        material_id: BeautyMaterialIdV17(1),
    };
    assert!(!puddle.is_ground_anchored());
}

#[test]
fn v17_rejects_camera_relative_human_proxy() {
    let mut human =
        HumanProxyV17::adult_world_anchored(1, BeautyCellIdV17(7), [0.0, 0.0, 0.02], 1234);
    human.anchor_mode = BeautyAnchorModeV17::CameraRelative;
    assert!(!human.is_proportionate_non_rod());
}

#[test]
fn v17_draw_list_contains_sky_human_and_vehicle_for_valid_scene() {
    let mut scene = valid_small_scene();
    scene.collect_cell_material_pages();
    let report = scene.validate_for_visual_realism();
    assert!(
        report.pass,
        "unexpected failures: {:?}",
        report.failure_reasons
    );

    let draw_list = BeautyDrawListV17::from_scene(&scene);
    assert!(draw_list.has_sky_commands());
    assert!(draw_list.has_world_anchored_human_and_vehicle());
    assert!(
        draw_list
            .commands
            .iter()
            .any(|cmd| cmd.kind == BeautyPrimitiveKindV17::RoadMesh)
    );
    assert!(
        draw_list
            .commands
            .iter()
            .any(|cmd| cmd.kind == BeautyPrimitiveKindV17::FacadeMesh)
    );
}

fn valid_small_scene() -> BeautySceneV17 {
    let cell_id = BeautyCellIdV17(1);
    let road_surface = BeautySurfaceIdV17(100);
    let facade_surface = BeautySurfaceIdV17(101);
    let mat_asphalt = BeautyMaterialIdV17(10);
    let mat_concrete = BeautyMaterialIdV17(11);
    let mat_curb = BeautyMaterialIdV17(12);
    let mat_water = BeautyMaterialIdV17(13);
    let mat_metal = BeautyMaterialIdV17(14);
    let bounds = BeautyBoundsV17 {
        min: [-8.0, -8.0, -0.1],
        max: [8.0, 8.0, 7.0],
    };

    let road = RoadPatchV17 {
        object_id: BeautyObjectIdV17(1),
        surface_id: road_surface,
        centerline_world: vec![[0.0, -7.0, 0.0], [0.2, 0.0, 0.02], [0.0, 7.0, 0.0]],
        width_meters: 5.0,
        crown_height_meters: 0.035,
        edge_noise_meters: 0.18,
        puddle_basin_count: 2,
        pothole_count: 1,
        material_id: mat_asphalt,
        irregularity: IrregularityRecipeV17::road(1),
    };

    let curb = CurbSegmentV17 {
        object_id: BeautyObjectIdV17(2),
        start_world: [-2.7, -7.0, 0.03],
        end_world: [-2.7, 7.0, 0.03],
        height_meters: 0.15,
        width_meters: 0.3,
        bevel_radius_meters: 0.04,
        chip_density_0_to_1: 0.3,
        material_id: mat_curb,
        irregularity: IrregularityRecipeV17::curb(2),
    };

    let facade = FacadeModuleV17 {
        object_id: BeautyObjectIdV17(3),
        bounds: BeautyBoundsV17 {
            min: [-7.5, -7.0, 0.0],
            max: [-3.2, 7.0, 6.0],
        },
        material_id: mat_concrete,
        window_count: 8,
        door_count: 1,
        vent_count: 3,
        pipe_mount_count: 2,
        sign_mount_count: 1,
        inset_depth_meters: 0.12,
        bevel_radius_meters: 0.045,
        facade_warp_meters: 0.012,
        dirt_0_to_1: 0.6,
        poster_or_stain_count: 5,
        irregularity: IrregularityRecipeV17::dirty_facade(3),
    };

    let pipe = CurveObjectV17 {
        object_id: BeautyObjectIdV17(4),
        kind: CurveObjectKindV17::Pipe,
        points_world: vec![[-3.0, -6.0, 2.4], [-3.1, 0.0, 2.5], [-3.0, 6.0, 2.4]],
        radius_meters: 0.055,
        material_id: mat_metal,
        sag_meters: 0.0,
        surface_dirt_0_to_1: 0.4,
    };

    let scatter = ScatterFieldV17 {
        object_id: BeautyObjectIdV17(5),
        bounds,
        density_0_to_1: 0.3,
        item_count_budget: 64,
        has_trash: true,
        has_stones: true,
        has_paper: true,
        has_broken_glass: true,
        has_cable_clutter: true,
        seed: 5,
    };

    let puddle = GroundedPuddleV17 {
        object_id: BeautyObjectIdV17(6),
        receiver_surface_id: road_surface,
        center_world: [0.3, 0.4, 0.004],
        receiver_normal_world: [0.0, 0.0, 1.0],
        radius_meters: 0.8,
        max_depth_meters: 0.02,
        edge_softness_meters: 0.15,
        material_id: mat_water,
    };

    let mut pages = MaterialPageRequestV17::wet_asphalt_pack(road_surface, mat_asphalt, 9);
    pages.extend(MaterialPageRequestV17::dirty_concrete_pack(
        facade_surface,
        mat_concrete,
        10,
    ));

    let cell = BeautyCellPackageV17 {
        cell_id,
        bounds,
        roads: vec![road],
        curbs: vec![curb],
        facades: vec![facade],
        pipes_and_cables: vec![pipe],
        scatter_fields: vec![scatter],
        puddles: vec![puddle],
        material_page_requests: pages,
        retained_cache_key: 1,
        dirty: true,
    };

    let mut scene = BeautySceneV17::new(0);
    scene.cells.push(cell);
    scene.humans.push(HumanProxyV17::adult_world_anchored(
        1,
        cell_id,
        [1.0, 1.0, 0.02],
        11,
    ));
    scene
        .vehicles
        .push(VehicleProxyV17::compact_car_world_anchored(
            2,
            cell_id,
            [-1.0, -1.0, 0.03],
            12,
        ));
    scene
}
