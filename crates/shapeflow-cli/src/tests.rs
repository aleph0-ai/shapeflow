use super::*;
use crate::materialization_metadata::MaterializationMetadataRecord;
use crate::site_metadata::SiteMetadataRecord;
use crate::split_assignments_metadata::SplitAssignmentsMetadataRecord;
use shapeflow_core::{
    LatentArtifact, SceneGenerationParams, SceneProjectionMode, TargetArtifact, canonical_scene_id,
    deserialize_latent_artifact, deserialize_site_graph_artifact, deserialize_target_artifact,
    expected_target_task_ids, extract_latent_vector_from_scene, generate_all_scene_targets,
    generate_scene, generate_scene_text_lines_with_scene_config, generate_tabular_motion_rows,
    render_scene_image_png_with_scene_config, render_scene_sound_wav,
    render_scene_video_frames_png_with_keyframe_border, serialize_latent_artifact,
    serialize_scene_text, serialize_site_graph_artifact, serialize_tabular_motion_rows_csv,
    serialize_target_artifact, validate_site_graph_with_artifact,
};
use std::path::Path;

#[test]
fn validate_landscape_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            1,
            24,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .is_ok(),
        "validate --landscape should succeed for bootstrap config"
    );
}

#[test]
fn validate_scene_generation_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            1,
            24,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .is_ok(),
        "validate --scene-generation should succeed for bootstrap config"
    );
}

#[test]
fn validate_targets_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            1,
            24,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .is_ok(),
        "validate --targets should succeed for bootstrap config"
    );
}

#[test]
fn validate_site_graph_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            1,
            24,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
        )
        .is_ok(),
        "validate --site-graph should succeed for bootstrap config"
    );
}

#[test]
fn validate_sound_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            1,
            24,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
        )
        .is_ok(),
        "validate --sound should succeed for bootstrap config"
    );
}

#[test]
fn validate_sound_multi_scene_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            3,
            24,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
        )
        .is_ok(),
        "validate --sound should succeed for multiple scenes on bootstrap config"
    );
}

#[test]
fn validate_generation_checks_reject_zero_scene_count() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            0,
            24,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
        )
        .is_err(),
        "validate --sound should reject scene_count=0"
    );
}

#[test]
fn validate_split_assignments_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            3,
            24,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
        )
        .is_ok(),
        "validate --split-assignments should succeed for bootstrap config"
    );
}

#[test]
fn validate_split_assignments_reject_zero_scene_count() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            0,
            24,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
        )
        .is_err(),
        "validate --split-assignments should reject scene_count=0"
    );
}

#[test]
fn validate_generated_split_assignments_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 3;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std =
        std::env::temp_dir().join(format!("shapeflow-validate-generated-split-{nanos}"));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");
    assert!(
        run_validate(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
        )
        .is_ok(),
        "validate --generated-split-assignments should succeed for generated bootstrap output"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_split_assignments_requires_generated_output() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    assert!(
        run_validate(
            config_path,
            None,
            3,
            24,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
        )
        .is_err(),
        "validate --generated-split-assignments should require --generated-output"
    );
}

#[test]
fn validate_generated_split_assignments_detects_summary_mismatch() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 3;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-split-mismatch-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    let split_path = output_path.join("metadata/split_assignments.toml");
    let split_raw = std::fs::read_to_string(split_path.as_std_path())
        .expect("split assignments metadata should be readable");
    let mut split_metadata: SplitAssignmentsMetadataRecord =
        toml::from_str(&split_raw).expect("split assignments metadata should parse");
    split_metadata.summary.test_count += 1;
    let tampered = toml::to_string_pretty(&split_metadata)
        .expect("tampered split assignments metadata should serialize");
    std::fs::write(split_path.as_std_path(), tampered)
        .expect("tampered split assignments metadata should be writable");

    assert!(
        run_validate(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
        )
        .is_err(),
        "validate --generated-split-assignments should fail when generated metadata summary is tampered"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_site_metadata_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-site-metadata-smoke-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");
    assert!(
        run_validate(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_ok(),
        "validate --generated-site-metadata should succeed for generated bootstrap output"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_site_metadata_requires_generated_output() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate(
            config_path,
            None,
            2,
            24,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_err(),
        "validate --generated-site-metadata should require --generated-output"
    );
}

#[test]
fn validate_generated_site_metadata_detects_metric_mismatch() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-site-metadata-mismatch-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    let metadata_path = output_path.join("metadata/site_metadata.toml");
    let metadata_raw = std::fs::read_to_string(metadata_path.as_std_path())
        .expect("site metadata should be readable");
    let mut metadata: SiteMetadataRecord =
        toml::from_str(&metadata_raw).expect("site metadata should parse");
    metadata.max_degree = metadata.max_degree.saturating_add(1);
    let tampered =
        toml::to_string_pretty(&metadata).expect("tampered site metadata should serialize");
    std::fs::write(metadata_path.as_std_path(), tampered)
        .expect("tampered site metadata should be writable");

    assert!(
        run_validate(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_err(),
        "validate --generated-site-metadata should fail when site metadata is tampered"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_site_graph_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-site-graph-smoke-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");
    assert!(
        run_validate(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
        )
        .is_ok(),
        "validate --generated-site-graph should succeed for generated bootstrap output"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_site_graph_requires_generated_output() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    let err = run_validate(
        config_path,
        None,
        2,
        24,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        true,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("generated_output is required when --generated-site-graph is set"),
        "validate --generated-site-graph should require --generated-output"
    );
}

#[test]
fn validate_generated_site_graph_detects_artifact_tamper() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-site-graph-tamper-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    let site_graph_path = output_path.join("metadata/site_graph.sfg");
    let mut site_graph_bytes = std::fs::read(site_graph_path.as_std_path())
        .expect("site graph artifact should be readable");
    if !site_graph_bytes.is_empty() {
        let last = site_graph_bytes.len() - 1;
        site_graph_bytes[last] = site_graph_bytes[last].wrapping_add(1);
    }
    std::fs::write(site_graph_path.as_std_path(), site_graph_bytes)
        .expect("tampered site graph artifact should be writable");

    assert!(
        run_validate(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
        )
        .is_err(),
        "validate --generated-site-graph should fail when site graph artifact is tampered"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_config_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std =
        std::env::temp_dir().join(format!("shapeflow-validate-generated-config-smoke-{nanos}"));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    assert!(
        run_validate_with_generated_materialization(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
        )
        .is_ok(),
        "validate --generated-config should succeed for generated bootstrap output"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_config_requires_generated_output() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");

    assert!(
        run_validate_with_generated_materialization(
            config_path,
            None,
            2,
            24,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
        )
        .is_err(),
        "validate --generated-config should require --generated-output"
    );
}

#[test]
fn validate_generated_config_detects_tamper() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-config-tamper-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    let generated_config_path = output_path.join("metadata/config.toml");
    let mut tampered_config =
        load_config(generated_config_path.clone()).expect("generated config metadata should load");
    tampered_config.scene.trajectory_complexity = 1;
    let tampered_raw =
        toml::to_string_pretty(&tampered_config).expect("tampered config should serialize");
    std::fs::write(generated_config_path.as_std_path(), tampered_raw)
        .expect("tampered config metadata should be writable");

    assert!(
        run_validate_with_generated_materialization(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
        )
        .is_err(),
        "validate --generated-config should fail when generated config metadata is tampered"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_materialization_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-materialization-smoke-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");
    assert!(
        run_validate_with_generated_materialization(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_ok(),
        "validate --generated-materialization should succeed for generated bootstrap output"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_materialization_requires_generated_output() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    assert!(
        run_validate_with_generated_materialization(
            config_path,
            None,
            2,
            24,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_err(),
        "validate --generated-materialization should require --generated-output"
    );
}

#[test]
fn validate_generated_materialization_detects_count_mismatch() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-materialization-mismatch-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    let materialization_path = output_path.join("metadata/materialization.toml");
    let materialization_raw = std::fs::read_to_string(materialization_path.as_std_path())
        .expect("materialization metadata should be readable");
    let mut materialization_metadata: MaterializationMetadataRecord =
        toml::from_str(&materialization_raw).expect("materialization metadata should parse");
    materialization_metadata.sound_file_count += 1;
    let tampered = toml::to_string_pretty(&materialization_metadata)
        .expect("tampered materialization metadata should serialize");
    std::fs::write(materialization_path.as_std_path(), tampered)
        .expect("tampered materialization metadata should be writable");

    assert!(
        run_validate_with_generated_materialization(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_err(),
        "validate --generated-materialization should fail when generated metadata is tampered"
    );
    std::fs::remove_dir_all(output_std).expect("temp output should be removable");
}

#[test]
fn validate_generated_materialization_detects_filename_tamper() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-materialization-filename-tamper-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    let scene_id = canonical_scene_id(0);
    let original_path = output_path.join("latent").join(format!("{scene_id}.bin"));
    let tampered_path = output_path
        .join("latent")
        .join(format!("{scene_id}_tampered.bin"));
    std::fs::rename(original_path.as_std_path(), tampered_path.as_std_path())
        .expect("latent artifact should be renameable for filename tamper simulation");

    assert!(
        run_validate_with_generated_materialization(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_err(),
        "validate --generated-materialization should fail when artifact filename is tampered"
    );
    std::fs::remove_dir_all(output_std).expect("smoke output dir should be removable");
}

#[test]
fn validate_generated_materialization_detects_target_payload_tamper() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-materialization-target-tamper-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    let scene_id = canonical_scene_id(0);
    let other_scene_id = canonical_scene_id(1);
    let replacement_path = output_path
        .join("targets")
        .join(format!("{other_scene_id}_oqp0000.sft"));
    let replacement = std::fs::read(replacement_path.as_std_path())
        .expect("replacement target artifact should be readable");
    let tampered_path = output_path
        .join("targets")
        .join(format!("{scene_id}_oqp0000.sft"));
    std::fs::write(tampered_path.as_std_path(), replacement)
        .expect("tampered target artifact should be writable");

    assert!(
        run_validate_with_generated_materialization(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_err(),
        "validate --generated-materialization should fail when target payload is tampered"
    );
    std::fs::remove_dir_all(output_std).expect("smoke output dir should be removable");
}

#[test]
fn validate_generated_materialization_detects_text_payload_tamper() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!(
        "shapeflow-validate-generated-materialization-text-tamper-{nanos}"
    ));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    run_generate(
        config_path.clone(),
        output_path.clone(),
        scene_count,
        samples_per_event,
    )
    .expect("generate should succeed for bootstrap config");

    let scene_id = canonical_scene_id(0);
    let other_scene_id = canonical_scene_id(1);
    let replacement_text = std::fs::read_to_string(
        output_path
            .join("text")
            .join(format!("{other_scene_id}.txt"))
            .as_std_path(),
    )
    .expect("replacement text artifact should be readable");
    let tampered_path = output_path.join("text").join(format!("{scene_id}.txt"));
    std::fs::write(tampered_path.as_std_path(), replacement_text)
        .expect("tampered text artifact should be writable");

    assert!(
        run_validate_with_generated_materialization(
            config_path,
            Some(output_path),
            scene_count,
            samples_per_event,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .is_err(),
        "validate --generated-materialization should fail when text payload is tampered"
    );
    std::fs::remove_dir_all(output_std).expect("smoke output dir should be removable");
}

#[test]
fn generate_smoke_bootstrap_config() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std = std::env::temp_dir().join(format!("shapeflow-generate-smoke-{nanos}"));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    let result = run_generate(
        config_path,
        output_path.clone(),
        scene_count,
        samples_per_event,
    );
    assert!(
        result.is_ok(),
        "generate should succeed for bootstrap config"
    );

    assert!(
        output_path
            .join("metadata/config.toml")
            .as_std_path()
            .exists(),
        "generate should write metadata/config.toml"
    );
    let output_config = load_config(output_path.join("metadata/config.toml"))
        .expect("generated metadata config should parse into ShapeFlowConfig");
    output_config
        .validate()
        .expect("generated metadata config should validate");
    assert!(
        output_path
            .join("metadata/site_graph.sfg")
            .as_std_path()
            .exists(),
        "generate should write metadata/site_graph.sfg"
    );
    assert!(
        output_path
            .join("metadata/site_metadata.toml")
            .as_std_path()
            .exists(),
        "generate should write metadata/site_metadata.toml"
    );
    assert!(
        output_path
            .join("metadata/materialization.toml")
            .as_std_path()
            .exists(),
        "generate should write metadata/materialization.toml"
    );
    assert!(
        output_path.join("latent").as_std_path().exists(),
        "generate should write latent directory"
    );
    assert!(
        output_path.join("tabular").as_std_path().exists(),
        "generate should write tabular directory"
    );
    assert!(
        output_path.join("text").as_std_path().exists(),
        "generate should write text directory"
    );
    assert!(
        output_path.join("image").as_std_path().exists(),
        "generate should write image directory"
    );
    assert!(
        output_path.join("video_frames").as_std_path().exists(),
        "generate should write video_frames directory"
    );
    assert!(
        output_path.join("sound").as_std_path().exists(),
        "generate should write sound directory"
    );
    assert!(
        output_path
            .join("metadata/split_assignments.toml")
            .as_std_path()
            .exists(),
        "generate should write metadata/split_assignments.toml"
    );

    let split_assignments = std::fs::read_to_string(
        output_path
            .join("metadata/split_assignments.toml")
            .as_std_path(),
    )
    .expect("split assignments metadata should be readable");
    let split_assignments_value: toml::Value =
        toml::from_str(&split_assignments).expect("split assignments metadata should parse");
    let split_assignments_count = split_assignments_value
        .get("summary")
        .and_then(|summary| summary.get("total_count"))
        .and_then(|value| value.as_integer())
        .expect("split assignments summary should include total_count");
    assert_eq!(
        split_assignments_count, 2,
        "split assignments total_count should be 2"
    );

    let assignments = split_assignments_value
        .get("assignments")
        .and_then(|value| value.as_array())
        .expect("split assignments should include assignments array");
    assert_eq!(
        assignments.len(),
        usize::try_from(scene_count).unwrap_or(0),
        "split assignments count should equal scene_count"
    );

    let first_assignment = assignments
        .first()
        .and_then(|entry| entry.as_table())
        .expect("assignments should include first table entry");
    let first_scene_id = first_assignment
        .get("scene_id")
        .and_then(|value| value.as_str())
        .expect("first assignment should have scene_id");
    let first_split = first_assignment
        .get("split")
        .and_then(|value| value.as_str())
        .expect("first assignment should have split");
    assert_eq!(first_scene_id, "scene_000000");
    assert_eq!(first_split, "train");

    let materialization = std::fs::read_to_string(
        output_path
            .join("metadata/materialization.toml")
            .as_std_path(),
    )
    .expect("materialization metadata should be readable");
    let materialization_value: toml::Value =
        toml::from_str(&materialization).expect("materialization metadata should parse");
    let latent_artifact_count = materialization_value
        .get("latent_artifact_count")
        .and_then(|value| value.as_integer())
        .expect("materialization metadata should include latent_artifact_count");
    assert_eq!(
        latent_artifact_count as usize,
        usize::try_from(scene_count).unwrap_or(0),
        "latent_artifact_count should equal scene_count"
    );
    let text_file_count_metadata = materialization_value
        .get("text_file_count")
        .and_then(|value| value.as_integer())
        .expect("materialization metadata should include text_file_count");
    assert_eq!(
        text_file_count_metadata as usize,
        usize::try_from(scene_count).unwrap_or(0),
        "text_file_count should equal scene_count"
    );
    let sound_file_count_metadata = materialization_value
        .get("sound_file_count")
        .and_then(|value| value.as_integer())
        .expect("materialization metadata should include sound_file_count");
    assert_eq!(
        sound_file_count_metadata as usize,
        usize::try_from(scene_count).unwrap_or(0),
        "sound_file_count should equal scene_count"
    );
    let image_file_count_metadata = materialization_value
        .get("image_file_count")
        .and_then(|value| value.as_integer())
        .expect("materialization metadata should include image_file_count");
    assert_eq!(
        image_file_count_metadata as usize,
        usize::try_from(scene_count).unwrap_or(0),
        "image_file_count should equal scene_count"
    );
    let expected_video_frames_per_scene = usize::try_from(output_config.scene.n_motion_slots)
        .unwrap_or(0)
        * usize::from(output_config.scene.event_duration_frames);
    let expected_video_frame_total =
        usize::try_from(scene_count).unwrap_or(0) * expected_video_frames_per_scene;
    let video_frame_file_count_metadata = materialization_value
        .get("video_frame_file_count")
        .and_then(|value| value.as_integer())
        .expect("materialization metadata should include video_frame_file_count");
    assert_eq!(
        video_frame_file_count_metadata as usize, expected_video_frame_total,
        "video_frame_file_count should equal expected per-scene frame total"
    );

    let target_dir = output_path.join("targets");
    let target_file_count = std::fs::read_dir(target_dir.as_std_path())
        .expect("targets dir should exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("sft"))
        .count();
    assert_eq!(
        target_file_count,
        usize::try_from(scene_count).unwrap_or(0)
            * expected_target_task_ids(usize::from(output_config.scene.n_shapes)).len(),
        "bootstrap config with scene_count scenes should write all target files per scene"
    );

    let latent_file_count = std::fs::read_dir(output_path.join("latent").as_std_path())
        .expect("latent dir should exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("bin"))
        .count();
    assert_eq!(
        latent_file_count,
        usize::try_from(scene_count).unwrap_or(0),
        "latent file count should equal scene_count"
    );

    let tabular_file_count = std::fs::read_dir(output_path.join("tabular").as_std_path())
        .expect("tabular dir should exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("csv"))
        .count();
    assert_eq!(
        tabular_file_count,
        usize::try_from(scene_count).unwrap_or(0),
        "tabular file count should equal scene_count"
    );
    let text_file_count = std::fs::read_dir(output_path.join("text").as_std_path())
        .expect("text dir should exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("txt"))
        .count();
    assert_eq!(
        text_file_count,
        usize::try_from(scene_count).unwrap_or(0),
        "text file count should equal scene_count"
    );
    let image_file_count = std::fs::read_dir(output_path.join("image").as_std_path())
        .expect("image dir should exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("png"))
        .count();
    assert_eq!(
        image_file_count,
        usize::try_from(scene_count).unwrap_or(0),
        "image file count should equal scene_count"
    );
    let sound_file_count = std::fs::read_dir(output_path.join("sound").as_std_path())
        .expect("sound dir should exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("wav"))
        .count();
    assert_eq!(
        sound_file_count,
        usize::try_from(scene_count).unwrap_or(0),
        "sound file count should equal scene_count"
    );
    let video_frame_file_count = std::fs::read_dir(output_path.join("video_frames").as_std_path())
        .expect("video_frames dir should exist")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .flat_map(|scene_dir| {
            std::fs::read_dir(scene_dir)
                .expect("scene video frame dir should be readable")
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .collect::<Vec<_>>()
        })
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("png"))
        .count();
    assert_eq!(
        video_frame_file_count, expected_video_frame_total,
        "video frame file count should match expected deterministic total"
    );

    std::fs::remove_dir_all(output_std).expect("smoke output dir should be removable");
}

#[test]
fn generate_smoke_bootstrap_config_matches_core_artifacts() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std =
        std::env::temp_dir().join(format!("shapeflow-generate-smoke-artifacts-{nanos}"));
    let output_path =
        Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp output path should be utf-8");

    let result = run_generate(
        config_path,
        output_path.clone(),
        scene_count,
        samples_per_event,
    );
    assert!(
        result.is_ok(),
        "generate should succeed for bootstrap config"
    );

    let output_config_path = output_path.join("metadata/config.toml");
    assert!(
        output_config_path.as_std_path().exists(),
        "run should write generated metadata/config.toml"
    );
    let output_config =
        load_config(output_config_path.clone()).expect("generated metadata config should parse");
    output_config
        .validate()
        .expect("generated metadata config should validate");

    let (_site_report, site_artifact) = validate_site_graph_with_artifact(&output_config)
        .expect("site graph validation should succeed");
    let expected_site_graph_bytes = serialize_site_graph_artifact(&site_artifact)
        .expect("generated site graph artifact serialization should succeed");
    let site_graph_path = output_path.join("metadata/site_graph.sfg");
    let site_graph_bytes = std::fs::read(site_graph_path.as_std_path())
        .expect("generated site graph should be readable");
    assert_eq!(
        site_graph_bytes, expected_site_graph_bytes,
        "site graph artifact bytes should match validate_site_graph_with_artifact output"
    );

    let decoded_site_graph = deserialize_site_graph_artifact(&site_graph_bytes)
        .expect("generated site graph should deserialize");
    assert_eq!(
        decoded_site_graph, site_artifact,
        "generated site graph artifact should decode to expected value"
    );

    for scene_index in 0..scene_count {
        let params = SceneGenerationParams {
            config: &output_config,
            scene_index: u64::from(scene_index),
            samples_per_event,
            projection: SceneProjectionMode::SoftQuadrants,
        };
        let output =
            generate_scene(&params).expect("scene should regenerate with deterministic params");
        let scene_id = canonical_scene_id(u64::from(scene_index));

        let latent_values =
            extract_latent_vector_from_scene(&output).expect("latent vector should extract");
        let expected_latent = LatentArtifact {
            schema_version: output_config.schema_version,
            scene_id: scene_id.clone(),
            values: latent_values,
        };
        let expected_latent_bytes = serialize_latent_artifact(&expected_latent)
            .expect("latent artifact serialization should succeed");
        let latent_path = output_path.join("latent").join(format!("{scene_id}.bin"));
        let latent_bytes =
            std::fs::read(latent_path.as_std_path()).expect("latent artifact should be readable");
        assert_eq!(
            latent_bytes, expected_latent_bytes,
            "latent artifact bytes should match generated output"
        );

        let decoded_latent =
            deserialize_latent_artifact(&latent_bytes).expect("latent artifact should deserialize");
        assert_eq!(
            decoded_latent, expected_latent,
            "latent artifact should roundtrip to expected value"
        );

        let expected_tabular_rows =
            generate_tabular_motion_rows(&output).expect("tabular rows should generate");
        let expected_tabular_csv = serialize_tabular_motion_rows_csv(&expected_tabular_rows);
        let tabular_path = output_path.join("tabular").join(format!("{scene_id}.csv"));
        let tabular_csv = std::fs::read_to_string(tabular_path.as_std_path())
            .expect("tabular CSV should be readable");
        assert_eq!(
            tabular_csv, expected_tabular_csv,
            "tabular CSV should match core-generated tabular serialization"
        );
        let expected_text_lines =
            generate_scene_text_lines_with_scene_config(&output, &output_config.scene)
                .expect("text lines should generate");
        let expected_text = serialize_scene_text(&expected_text_lines);
        let text_path = output_path.join("text").join(format!("{scene_id}.txt"));
        let text = std::fs::read_to_string(text_path.as_std_path())
            .expect("text artifact should be readable");
        assert_eq!(
            text, expected_text,
            "text artifact should match core-generated text serialization"
        );
        let expected_image =
            render_scene_image_png_with_scene_config(&output, &output_config.scene)
                .expect("image should render");
        let image_path = output_path.join("image").join(format!("{scene_id}.png"));
        let image_bytes =
            std::fs::read(image_path.as_std_path()).expect("image artifact should be readable");
        assert_eq!(
            image_bytes, expected_image,
            "image artifact should match core-generated image serialization"
        );
        let expected_sound = render_scene_sound_wav(
            &output,
            output_config.scene.sound_sample_rate_hz,
            output_config.scene.sound_frames_per_second,
            output_config.scene.sound_modulation_depth_per_mille,
            output_config.scene.sound_channel_mapping,
        )
        .expect("sound should render");
        let sound_path = output_path.join("sound").join(format!("{scene_id}.wav"));
        let sound_bytes =
            std::fs::read(sound_path.as_std_path()).expect("sound artifact should be readable");
        assert_eq!(
            sound_bytes, expected_sound,
            "sound artifact should match core-generated sound serialization for scene {scene_id}"
        );
        let expected_video_frames = render_scene_video_frames_png_with_keyframe_border(
            &output,
            output_config.scene.resolution,
            output_config.scene.video_keyframe_border,
        )
        .expect("video frames should render");
        let video_scene_dir = output_path.join("video_frames").join(&scene_id);
        for (frame_index, expected_frame) in expected_video_frames.iter().enumerate() {
            let frame_path = video_scene_dir.join(format!("frame_{frame_index:06}.png"));
            let frame_bytes =
                std::fs::read(frame_path.as_std_path()).expect("video frame should be readable");
            assert_eq!(
                frame_bytes, *expected_frame,
                "video frame should match core-generated frame serialization for scene {scene_id}, frame {frame_index}"
            );
        }

        let mut targets = generate_all_scene_targets(&output).expect("targets should generate");
        targets.sort_by(|left, right| left.task_id.cmp(&right.task_id));
        for target in targets {
            let task_id = target.task_id.clone();
            let expected_target = TargetArtifact {
                schema_version: output_config.schema_version,
                scene_id: scene_id.clone(),
                task_id: task_id.clone(),
                segments: target.segments,
            };
            let expected_target_bytes = serialize_target_artifact(&expected_target)
                .expect("target artifact serialization should succeed");
            let target_path = output_path
                .join("targets")
                .join(format!("{scene_id}_{task_id}.sft"));
            let target_bytes = std::fs::read(target_path.as_std_path())
                .expect("target artifact should be readable");
            assert_eq!(
                target_bytes, expected_target_bytes,
                "target artifact bytes should match generated output"
            );

            let decoded_target = deserialize_target_artifact(&target_bytes)
                .expect("target artifact should deserialize");
            assert_eq!(
                decoded_target, expected_target,
                "target artifact should roundtrip to expected value"
            );
        }
    }

    std::fs::remove_dir_all(output_std).expect("smoke output dir should be removable");
}

#[test]
fn generate_bootstrap_config_is_deterministic() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = Utf8PathBuf::from_path_buf(
        manifest_dir
            .join("../../configs/bootstrap.toml")
            .to_path_buf(),
    )
    .expect("bootstrap config path should be utf-8");
    let scene_count: u32 = 2;
    let samples_per_event: usize = 24;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let output_std_1 =
        std::env::temp_dir().join(format!("shapeflow-generate-deterministic-a-{nanos}"));
    let output_std_2 =
        std::env::temp_dir().join(format!("shapeflow-generate-deterministic-b-{nanos}"));
    let output_path_1 =
        Utf8PathBuf::from_path_buf(output_std_1.clone()).expect("temp output path should be utf-8");
    let output_path_2 =
        Utf8PathBuf::from_path_buf(output_std_2.clone()).expect("temp output path should be utf-8");

    let result_1 = run_generate(
        config_path.clone(),
        output_path_1.clone(),
        scene_count,
        samples_per_event,
    );
    assert!(
        result_1.is_ok(),
        "first deterministic generate should succeed for bootstrap config"
    );

    let result_2 = run_generate(
        config_path,
        output_path_2.clone(),
        scene_count,
        samples_per_event,
    );
    assert!(
        result_2.is_ok(),
        "second deterministic generate should succeed for bootstrap config"
    );

    let metadata_1_site_graph =
        std::fs::read(output_path_1.join("metadata/site_graph.sfg").as_std_path())
            .expect("first site graph artifact should be readable");
    let metadata_2_site_graph =
        std::fs::read(output_path_2.join("metadata/site_graph.sfg").as_std_path())
            .expect("second site graph artifact should be readable");
    assert_eq!(
        metadata_1_site_graph, metadata_2_site_graph,
        "site_graph.sfg should be identical across deterministic runs"
    );

    let metadata_1_materialization = std::fs::read(
        output_path_1
            .join("metadata/materialization.toml")
            .as_std_path(),
    )
    .expect("first materialization metadata should be readable");
    let metadata_2_materialization = std::fs::read(
        output_path_2
            .join("metadata/materialization.toml")
            .as_std_path(),
    )
    .expect("second materialization metadata should be readable");
    assert_eq!(
        metadata_1_materialization, metadata_2_materialization,
        "materialization.toml should be identical across deterministic runs"
    );

    let output_config = load_config(output_path_1.join("metadata/config.toml"))
        .expect("generated config should parse");
    let n_shapes = output_config.scene.n_shapes as u32;
    for scene_index in 0..scene_count {
        let scene_id = canonical_scene_id(u64::from(scene_index));

        let latent_path_1 = output_path_1.join("latent").join(format!("{scene_id}.bin"));
        let latent_path_2 = output_path_2.join("latent").join(format!("{scene_id}.bin"));
        assert_eq!(
            std::fs::read(latent_path_1.as_std_path())
                .expect("first latent artifact should be readable"),
            std::fs::read(latent_path_2.as_std_path())
                .expect("second latent artifact should be readable"),
            "latent artifact should match for scene {scene_id}"
        );

        let tabular_path_1 = output_path_1
            .join("tabular")
            .join(format!("{scene_id}.csv"));
        let tabular_path_2 = output_path_2
            .join("tabular")
            .join(format!("{scene_id}.csv"));
        assert_eq!(
            std::fs::read_to_string(tabular_path_1.as_std_path())
                .expect("first tabular artifact should be readable"),
            std::fs::read_to_string(tabular_path_2.as_std_path())
                .expect("second tabular artifact should be readable"),
            "tabular artifact should match for scene {scene_id}"
        );
        let text_path_1 = output_path_1.join("text").join(format!("{scene_id}.txt"));
        let text_path_2 = output_path_2.join("text").join(format!("{scene_id}.txt"));
        assert_eq!(
            std::fs::read_to_string(text_path_1.as_std_path())
                .expect("first text artifact should be readable"),
            std::fs::read_to_string(text_path_2.as_std_path())
                .expect("second text artifact should be readable"),
            "text artifact should match for scene {scene_id}"
        );
        let image_path_1 = output_path_1.join("image").join(format!("{scene_id}.png"));
        let image_path_2 = output_path_2.join("image").join(format!("{scene_id}.png"));
        assert_eq!(
            std::fs::read(image_path_1.as_std_path())
                .expect("first image artifact should be readable"),
            std::fs::read(image_path_2.as_std_path())
                .expect("second image artifact should be readable"),
            "image artifact should match for scene {scene_id}"
        );
        let sound_path_1 = output_path_1.join("sound").join(format!("{scene_id}.wav"));
        let sound_path_2 = output_path_2.join("sound").join(format!("{scene_id}.wav"));
        assert_eq!(
            std::fs::read(sound_path_1.as_std_path())
                .expect("first sound artifact should be readable"),
            std::fs::read(sound_path_2.as_std_path())
                .expect("second sound artifact should be readable"),
            "sound artifact should match for scene {scene_id}"
        );
        let video_scene_dir_1 = output_path_1.join("video_frames").join(&scene_id);
        let video_scene_dir_2 = output_path_2.join("video_frames").join(&scene_id);
        let video_frame_files_1 = std::fs::read_dir(video_scene_dir_1.as_std_path())
            .expect("first video frame directory should be readable")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("png"))
            .collect::<Vec<_>>();
        let video_frame_files_2 = std::fs::read_dir(video_scene_dir_2.as_std_path())
            .expect("second video frame directory should be readable")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("png"))
            .collect::<Vec<_>>();
        assert_eq!(
            video_frame_files_1.len(),
            video_frame_files_2.len(),
            "video frame count should match for scene {scene_id}"
        );
        for frame_index in 0..video_frame_files_1.len() {
            let frame_path_1 = video_scene_dir_1.join(format!("frame_{frame_index:06}.png"));
            let frame_path_2 = video_scene_dir_2.join(format!("frame_{frame_index:06}.png"));
            assert_eq!(
                std::fs::read(frame_path_1.as_std_path())
                    .expect("first video frame should be readable"),
                std::fs::read(frame_path_2.as_std_path())
                    .expect("second video frame should be readable"),
                "video frame should match for scene {scene_id}, frame {frame_index}"
            );
        }

        for task_id in expected_target_task_ids(usize::try_from(n_shapes).unwrap_or(0)) {
            let target_filename = format!("{scene_id}_{task_id}.sft");
            let target_path_1 = output_path_1.join("targets").join(&target_filename);
            let target_path_2 = output_path_2.join("targets").join(&target_filename);
            assert_eq!(
                std::fs::read(target_path_1.as_std_path())
                    .expect("first target artifact should be readable"),
                std::fs::read(target_path_2.as_std_path())
                    .expect("second target artifact should be readable"),
                "target artifact should match for {target_filename}"
            );
        }
    }

    std::fs::remove_dir_all(output_std_1).expect("first smoke output dir should be removable");
    std::fs::remove_dir_all(output_std_2).expect("second smoke output dir should be removable");
}
