use crate::generated_config_metadata::validate_generated_config_metadata;
use crate::load_config;
use crate::materialization_metadata::validate_generated_materialization_metadata;
use crate::site_graph_artifact::validate_generated_site_graph_artifact;
use crate::site_metadata::validate_generated_site_metadata;
use crate::split_assignments_metadata::validate_generated_split_assignments_metadata;
use anyhow::{Context, Result, ensure};
use camino::Utf8PathBuf;
use shapeflow_core::{
    SceneGenerationParams, SceneProjectionMode, TargetArtifact, build_split_assignments,
    deserialize_site_graph_artifact, deserialize_target_artifact,
    generate_ordered_quadrant_passage_targets, generate_scene,
    landscape_validation::validate_empirical_landscape, serialize_site_graph_artifact,
    serialize_target_artifact, validate_ordered_quadrant_passage_targets, validate_scene_sound_wav,
    validate_site_graph_with_artifact,
};

#[allow(dead_code)]
pub(crate) fn run_validate(
    config_path: Utf8PathBuf,
    generated_output: Option<Utf8PathBuf>,
    scene_count: u32,
    samples_per_event: usize,
    landscape: bool,
    scene_generation: bool,
    targets: bool,
    site_graph: bool,
    sound: bool,
    split_assignments: bool,
    generated_split_assignments: bool,
    generated_site_metadata: bool,
    generated_site_graph: bool,
) -> Result<()> {
    run_validate_with_generated_materialization(
        config_path,
        generated_output,
        scene_count,
        samples_per_event,
        landscape,
        scene_generation,
        targets,
        site_graph,
        sound,
        split_assignments,
        generated_split_assignments,
        generated_site_metadata,
        generated_site_graph,
        false,
        false,
    )
}

#[allow(dead_code)]
pub(crate) fn run_validate_with_generated_materialization(
    config_path: Utf8PathBuf,
    generated_output: Option<Utf8PathBuf>,
    scene_count: u32,
    samples_per_event: usize,
    landscape: bool,
    scene_generation: bool,
    targets: bool,
    site_graph: bool,
    sound: bool,
    split_assignments: bool,
    generated_split_assignments: bool,
    generated_site_metadata: bool,
    generated_site_graph: bool,
    generated_materialization: bool,
    generated_config: bool,
) -> Result<()> {
    let config = load_config(config_path)?;
    config.validate()?;

    let mut ran_specific_check = false;
    let mut generated_scenes = Vec::new();

    if site_graph {
        let (report, artifact) =
            validate_site_graph_with_artifact(&config).context("site graph validation failed")?;
        let bytes = serialize_site_graph_artifact(&artifact)
            .context("site graph artifact serialization failed")?;
        let decoded = deserialize_site_graph_artifact(&bytes)
            .context("site graph artifact deserialization failed")?;
        ensure!(
            decoded == artifact,
            "site graph artifact roundtrip mismatch after serialization"
        );

        println!("validation=site-graph-ok");
        println!(
            "scene_count={}, site_k={}, effective_k={}, undirected_edge_count={}, connected_components={}, min_degree={}, max_degree={}, mean_degree={:.6}, lambda2_estimate={:.6}, sfg_bytes={}",
            report.scene_count,
            report.site_k,
            report.effective_k,
            report.undirected_edge_count,
            report.connected_components,
            report.min_degree,
            report.max_degree,
            report.mean_degree,
            report.lambda2_estimate,
            bytes.len()
        );
        ran_specific_check = true;
    }

    if landscape {
        let report = validate_empirical_landscape(&config)
            .context("empirical landscape validation failed")?;
        println!("validation=landscape-ok");
        println!(
            "k2_avg_distinct_dominant_quadrants={:.6}, k3_corner_reachability_rate={:.6}, samples_per_check={}",
            report.k2_average_distinct_dominant_quadrants,
            report.k3_corner_reachability_rate,
            report.samples_per_check
        );
        ran_specific_check = true;
    }

    if scene_generation || targets || sound {
        ensure!(scene_count > 0, "scene_count must be > 0");
        ensure!(samples_per_event > 0, "samples_per_event must be > 0");

        let projection = if targets {
            SceneProjectionMode::SoftQuadrants
        } else {
            SceneProjectionMode::TrajectoryOnly
        };
        generated_scenes = (0..scene_count)
            .map(|scene_index| {
                let params = SceneGenerationParams {
                    config: &config,
                    scene_index: u64::from(scene_index),
                    samples_per_event,
                    projection,
                };
                generate_scene(&params).with_context(|| {
                    format!("scene generation validation failed for scene_index={scene_index}")
                })
            })
            .collect::<Result<Vec<_>>>()?;
    }

    if scene_generation {
        let expected_total: u32 = generated_scenes
            .iter()
            .map(|output| output.accounting.expected_total)
            .sum();
        let generated_total: u32 = generated_scenes
            .iter()
            .map(|output| output.accounting.generated_total)
            .sum();
        println!("validation=scene-generation-ok");
        println!(
            "validated_scene_count={}, samples_per_event={}, expected_total_events={}, generated_total_events={}",
            generated_scenes.len(),
            samples_per_event,
            expected_total,
            generated_total
        );
        ran_specific_check = true;
    }

    if targets {
        let mut total_shape_targets = 0usize;
        let mut total_segments = 0usize;
        let mut hard_segments = 0usize;
        let mut total_target_bytes = 0usize;
        for output in &generated_scenes {
            let targets = generate_ordered_quadrant_passage_targets(output).with_context(|| {
                format!(
                    "ordered quadrant target generation failed for scene_index={}",
                    output.scene_index
                )
            })?;
            let report =
                validate_ordered_quadrant_passage_targets(&targets).with_context(|| {
                    format!(
                        "ordered quadrant target validation failed for scene_index={}",
                        output.scene_index
                    )
                })?;
            total_shape_targets += report.shape_target_count;
            total_segments += report.total_segments;
            hard_segments += report.hard_segment_count;

            let scene_id = format!("{:032x}", output.scene_index);
            for target in &targets {
                let task_id = format!("oqp{:04}", target.shape_index);
                let artifact = TargetArtifact {
                    schema_version: config.schema_version,
                    scene_id: scene_id.clone(),
                    task_id,
                    segments: target.segments.clone(),
                };
                let bytes = serialize_target_artifact(&artifact)
                    .context("target artifact serialization failed")?;
                let decoded = deserialize_target_artifact(&bytes)
                    .context("target artifact deserialization failed")?;
                ensure!(
                    decoded == artifact,
                    "target artifact roundtrip mismatch after serialization"
                );
                total_target_bytes += bytes.len();
            }
        }

        println!("validation=targets-ok");
        println!(
            "validated_scene_count={}, shape_targets={}, total_segments={}, hard_segments={}, sft_total_bytes={}",
            generated_scenes.len(),
            total_shape_targets,
            total_segments,
            hard_segments,
            total_target_bytes
        );
        ran_specific_check = true;
    }

    if sound {
        let mut interleaved_sample_count = 0usize;
        let mut samples_per_channel = 0usize;
        let mut expected_samples_per_channel = 0usize;
        let mut wav_bytes = 0usize;
        let channel_count = match config.scene.sound_channel_mapping {
            shapeflow_core::SoundChannelMapping::MonoMix => 1usize,
            shapeflow_core::SoundChannelMapping::StereoAlternating => 2usize,
        };
        for output in &generated_scenes {
            let report = validate_scene_sound_wav(
                output,
                config.scene.sound_sample_rate_hz,
                config.scene.sound_frames_per_second,
                config.scene.sound_modulation_depth_per_mille,
                config.scene.sound_channel_mapping,
            )
            .with_context(|| {
                format!(
                    "sound validation failed for scene_index={}",
                    output.scene_index
                )
            })?;
            interleaved_sample_count += report.interleaved_sample_count;
            samples_per_channel += report.samples_per_channel;
            expected_samples_per_channel += report.expected_samples_per_channel;
            wav_bytes += report.wav_byte_count;
        }
        println!("validation=sound-ok");
        println!(
            "validated_scene_count={}, channels={}, sample_rate_hz={}, interleaved_samples={}, samples_per_channel={}, expected_samples_per_channel={}, wav_bytes={}",
            generated_scenes.len(),
            channel_count,
            config.scene.sound_sample_rate_hz,
            interleaved_sample_count,
            samples_per_channel,
            expected_samples_per_channel,
            wav_bytes
        );
        ran_specific_check = true;
    }

    if split_assignments {
        ensure!(scene_count > 0, "scene_count must be > 0");
        let result = build_split_assignments(scene_count as usize).with_context(|| {
            format!("split-assignment validation failed for scene_count={scene_count}")
        })?;
        ensure!(
            result.assignments.len() == result.summary.total_count,
            "split assignment summary total_count does not match assignment length: summary_total={}, assignments_len={}",
            result.summary.total_count,
            result.assignments.len()
        );
        ensure!(
            result.summary.train_count + result.summary.val_count + result.summary.test_count
                == result.summary.total_count,
            "split assignment summary counts do not sum to total_count: train={}, val={}, test={}, total={}",
            result.summary.train_count,
            result.summary.val_count,
            result.summary.test_count,
            result.summary.total_count
        );
        println!("validation=split-assignments-ok");
        println!(
            "validated_scene_count={}, train_count={}, val_count={}, test_count={}, total_count={}",
            scene_count,
            result.summary.train_count,
            result.summary.val_count,
            result.summary.test_count,
            result.summary.total_count,
        );
        ran_specific_check = true;
    }

    if generated_split_assignments {
        ensure!(scene_count > 0, "scene_count must be > 0");
        let output_root = generated_output
            .as_ref()
            .context("generated_output must be provided for --generated-split-assignments")?;
        let validated_metadata = validate_generated_split_assignments_metadata(
            output_root,
            &config,
            scene_count as usize,
        )
        .with_context(|| {
            format!(
                "generated split-assignment metadata validation failed for output_root={}",
                output_root.as_str()
            )
        })?;
        println!("validation=generated-split-assignments-ok");
        println!(
            "validated_scene_count={}, train_count={}, val_count={}, test_count={}, total_count={}, output_root={}",
            scene_count,
            validated_metadata.summary.train_count,
            validated_metadata.summary.val_count,
            validated_metadata.summary.test_count,
            validated_metadata.summary.total_count,
            output_root.as_str()
        );
        ran_specific_check = true;
    }

    if generated_site_metadata {
        let output_root = generated_output
            .as_ref()
            .context("generated_output must be provided for --generated-site-metadata")?;
        let validated_metadata = validate_generated_site_metadata(output_root, &config)
            .with_context(|| {
                format!(
                    "generated site metadata validation failed for output_root={}",
                    output_root.as_str()
                )
            })?;
        println!("validation=generated-site-metadata-ok");
        println!(
            "scene_count={}, site_k={}, effective_k={}, undirected_edge_count={}, connected_components={}, min_degree={}, max_degree={}, mean_degree={:.6}, lambda2_estimate={:.6}, output_root={}",
            validated_metadata.scene_count,
            validated_metadata.site_k,
            validated_metadata.effective_k,
            validated_metadata.undirected_edge_count,
            validated_metadata.connected_components,
            validated_metadata.min_degree,
            validated_metadata.max_degree,
            validated_metadata.mean_degree,
            validated_metadata.lambda2_estimate,
            output_root.as_str()
        );
        ran_specific_check = true;
    }

    if generated_site_graph {
        let output_root = generated_output
            .as_ref()
            .context("generated_output is required when --generated-site-graph is set")?;
        let validated_record = validate_generated_site_graph_artifact(output_root, &config)
            .with_context(|| {
                format!(
                    "generated site-graph validation failed for output_root={}",
                    output_root.as_str()
                )
            })?;
        debug_assert_eq!(
            validated_record.generated_artifact, validated_record.recomputed_artifact,
            "generated and recomputed site-graph artifacts should match"
        );
        println!("validation=generated-site-graph-ok");
        println!(
            "scene_count={}, node_count={}, undirected_edge_count={}, effective_k={}, lambda2_estimate={:.6}, output_root={}",
            validated_record.recomputed_report.scene_count,
            validated_record.generated_artifact.node_count,
            validated_record.recomputed_report.undirected_edge_count,
            validated_record.recomputed_report.effective_k,
            validated_record.recomputed_report.lambda2_estimate,
            output_root.as_str()
        );
        ran_specific_check = true;
    }

    if generated_config {
        let output_root = generated_output
            .as_ref()
            .context("generated_output must be provided for --generated-config")?;
        let validated_config_metadata = validate_generated_config_metadata(output_root, &config)
            .with_context(|| {
                format!(
                    "generated config metadata validation failed for output_root={}",
                    output_root.as_str()
                )
            })?;
        println!("validation=generated-config-ok");
        println!(
            "schema_version={}, master_seed={}, config_hash={}, output_root={}",
            validated_config_metadata.schema_version,
            validated_config_metadata.master_seed,
            validated_config_metadata.config_hash,
            output_root.as_str()
        );
        ran_specific_check = true;
    }

    if generated_materialization {
        ensure!(scene_count > 0, "scene_count must be > 0");
        ensure!(samples_per_event > 0, "samples_per_event must be > 0");
        let output_root = generated_output
            .as_ref()
            .context("generated_output must be provided for --generated-materialization")?;
        let validated_metadata = validate_generated_materialization_metadata(
            output_root,
            &config,
            scene_count,
            samples_per_event,
        )
        .with_context(|| {
            format!(
                "generated materialization metadata validation failed for output_root={}",
                output_root.as_str()
            )
        })?;
        println!("validation=generated-materialization-ok");
        println!(
            "validated_scene_count={}, samples_per_event={}, target_file_count={}, total_target_segments={}, latent_artifact_count={}, sound_file_count={}, tabular_file_count={}, text_file_count={}, image_file_count={}, video_frame_file_count={}, output_root={}",
            validated_metadata.scene_count,
            validated_metadata.samples_per_event,
            validated_metadata.target_file_count,
            validated_metadata.total_target_segments,
            validated_metadata.latent_artifact_count,
            validated_metadata.sound_file_count,
            validated_metadata.tabular_file_count,
            validated_metadata.text_file_count,
            validated_metadata.image_file_count,
            validated_metadata.video_frame_file_count,
            output_root.as_str()
        );
        ran_specific_check = true;
    }

    if !ran_specific_check {
        println!("validation=ok");
    }
    Ok(())
}
