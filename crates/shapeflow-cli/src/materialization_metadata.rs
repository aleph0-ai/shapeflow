use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use shapeflow_core::{
    LatentArtifact, SceneGenerationParams, SceneProjectionMode, ShapeFlowConfig,
    canonical_scene_id, deserialize_latent_artifact, deserialize_target_artifact,
    expected_target_task_ids, extract_latent_vector_from_scene, generate_all_scene_targets,
    generate_scene, generate_scene_text_lines_with_scene_config, generate_tabular_motion_rows,
    render_scene_image_png_with_scene_config, render_scene_sound_wav,
    render_scene_video_frames_png_with_keyframe_border, serialize_latent_artifact,
    serialize_scene_text, serialize_tabular_motion_rows_csv,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MaterializationMetadataRecord {
    pub(crate) master_seed: u64,
    pub(crate) config_hash: String,
    pub(crate) schema_version: u32,
    pub(crate) scene_count: u32,
    pub(crate) samples_per_event: usize,
    pub(crate) target_file_count: usize,
    pub(crate) total_target_segments: usize,
    pub(crate) latent_artifact_count: usize,
    pub(crate) sound_file_count: usize,
    pub(crate) tabular_file_count: usize,
    pub(crate) text_file_count: usize,
    pub(crate) image_file_count: usize,
    pub(crate) video_frame_file_count: usize,
}

pub(crate) fn validate_generated_materialization_metadata(
    output_root: &Utf8Path,
    config: &ShapeFlowConfig,
    scene_count: u32,
    samples_per_event: usize,
) -> Result<MaterializationMetadataRecord> {
    let metadata_path = output_root.join("metadata/materialization.toml");
    let metadata_raw = std::fs::read_to_string(metadata_path.as_std_path()).with_context(|| {
        format!(
            "failed to read generated materialization metadata at {}",
            metadata_path.as_str()
        )
    })?;
    let metadata: MaterializationMetadataRecord =
        toml::from_str(&metadata_raw).with_context(|| {
            format!(
                "failed to parse generated materialization metadata TOML at {}",
                metadata_path.as_str()
            )
        })?;

    let identity = config
        .dataset_identity()
        .context("failed to compute dataset identity from config")?;
    ensure!(
        metadata.master_seed == identity.master_seed,
        "generated materialization metadata master_seed mismatch: file={}, expected={}",
        metadata.master_seed,
        identity.master_seed
    );
    ensure!(
        metadata.config_hash == identity.config_hash_hex,
        "generated materialization metadata config_hash mismatch: file={}, expected={}",
        metadata.config_hash,
        identity.config_hash_hex
    );
    ensure!(
        metadata.schema_version == config.schema_version,
        "generated materialization metadata schema_version mismatch: file={}, expected={}",
        metadata.schema_version,
        config.schema_version
    );
    ensure!(
        metadata.scene_count == scene_count,
        "generated materialization metadata scene_count mismatch: file={}, expected={}",
        metadata.scene_count,
        scene_count
    );
    ensure!(
        metadata.samples_per_event == samples_per_event,
        "generated materialization metadata samples_per_event mismatch: file={}, expected={}",
        metadata.samples_per_event,
        samples_per_event
    );

    let effective_scene_configs = vec![config.clone(); scene_count as usize];

    let expected_scene_count = scene_count as usize;
    let expected_target_file_count: usize = effective_scene_configs
        .iter()
        .map(|effective_config| {
            expected_target_task_ids(usize::from(effective_config.scene.n_shapes)).len()
        })
        .sum();
    let expected_video_frames_per_scene = effective_scene_configs
        .iter()
        .map(expected_video_frames_per_scene)
        .collect::<Result<Vec<_>>>()?;
    let expected_video_frame_file_count: usize = expected_video_frames_per_scene.iter().sum();

    ensure!(
        metadata.target_file_count == expected_target_file_count,
        "generated materialization metadata target_file_count mismatch: file={}, expected={}",
        metadata.target_file_count,
        expected_target_file_count
    );
    ensure!(
        metadata.latent_artifact_count == expected_scene_count,
        "generated materialization metadata latent_artifact_count mismatch: file={}, expected={}",
        metadata.latent_artifact_count,
        expected_scene_count
    );
    ensure!(
        metadata.sound_file_count == expected_scene_count,
        "generated materialization metadata sound_file_count mismatch: file={}, expected={}",
        metadata.sound_file_count,
        expected_scene_count
    );
    ensure!(
        metadata.tabular_file_count == expected_scene_count,
        "generated materialization metadata tabular_file_count mismatch: file={}, expected={}",
        metadata.tabular_file_count,
        expected_scene_count
    );
    ensure!(
        metadata.text_file_count == expected_scene_count,
        "generated materialization metadata text_file_count mismatch: file={}, expected={}",
        metadata.text_file_count,
        expected_scene_count
    );
    ensure!(
        metadata.image_file_count == expected_scene_count,
        "generated materialization metadata image_file_count mismatch: file={}, expected={}",
        metadata.image_file_count,
        expected_scene_count
    );
    ensure!(
        metadata.video_frame_file_count == expected_video_frame_file_count,
        "generated materialization metadata video_frame_file_count mismatch: file={}, expected={}",
        metadata.video_frame_file_count,
        expected_video_frame_file_count
    );

    let scene_ids = expected_scene_ids(scene_count);

    let latent_dir = output_root.join("latent");
    let tabular_dir = output_root.join("tabular");
    let text_dir = output_root.join("text");
    let image_dir = output_root.join("image");
    let sound_dir = output_root.join("sound");

    let observed_latent_files = ensure_exact_file_set(
        &latent_dir,
        &expected_artifact_filenames(&scene_ids, "bin"),
        "latent",
    )?;
    let observed_tabular_files = ensure_exact_file_set(
        &tabular_dir,
        &expected_artifact_filenames(&scene_ids, "csv"),
        "tabular",
    )?;
    let observed_text_files = ensure_exact_file_set(
        &text_dir,
        &expected_artifact_filenames(&scene_ids, "txt"),
        "text",
    )?;
    let observed_image_files = ensure_exact_file_set(
        &image_dir,
        &expected_artifact_filenames(&scene_ids, "png"),
        "image",
    )?;
    let observed_sound_files = ensure_exact_file_set(
        &sound_dir,
        &expected_artifact_filenames(&scene_ids, "wav"),
        "sound",
    )?;

    let video_frames_dir = output_root.join("video_frames");
    let observed_video_frame_file_count = validate_video_frame_file_sets(
        &video_frames_dir,
        &scene_ids,
        &expected_video_frames_per_scene,
    )?;

    let targets_dir = output_root.join("targets");
    let (observed_target_file_count, observed_target_segment_count) = validate_target_artifacts(
        &targets_dir,
        &effective_scene_configs,
        &scene_ids,
        samples_per_event,
    )?;
    validate_non_target_artifact_payloads(
        output_root,
        &effective_scene_configs,
        &scene_ids,
        samples_per_event,
    )
    .context("generated non-target artifact payload validation failed")?;

    ensure!(
        metadata.target_file_count == observed_target_file_count,
        "generated materialization metadata target_file_count does not match observed target files: metadata={}, observed={}",
        metadata.target_file_count,
        observed_target_file_count
    );
    ensure!(
        metadata.total_target_segments == observed_target_segment_count,
        "generated materialization metadata total_target_segments does not match observed decoded target segments: metadata={}, observed={}",
        metadata.total_target_segments,
        observed_target_segment_count
    );
    ensure!(
        metadata.latent_artifact_count == observed_latent_files.len(),
        "generated materialization metadata latent_artifact_count does not match observed latent files: metadata={}, observed={}",
        metadata.latent_artifact_count,
        observed_latent_files.len()
    );
    ensure!(
        metadata.sound_file_count == observed_sound_files.len(),
        "generated materialization metadata sound_file_count does not match observed sound files: metadata={}, observed={}",
        metadata.sound_file_count,
        observed_sound_files.len()
    );
    ensure!(
        metadata.tabular_file_count == observed_tabular_files.len(),
        "generated materialization metadata tabular_file_count does not match observed tabular files: metadata={}, observed={}",
        metadata.tabular_file_count,
        observed_tabular_files.len()
    );
    ensure!(
        metadata.text_file_count == observed_text_files.len(),
        "generated materialization metadata text_file_count does not match observed text files: metadata={}, observed={}",
        metadata.text_file_count,
        observed_text_files.len()
    );
    ensure!(
        metadata.image_file_count == observed_image_files.len(),
        "generated materialization metadata image_file_count does not match observed image files: metadata={}, observed={}",
        metadata.image_file_count,
        observed_image_files.len()
    );
    ensure!(
        metadata.video_frame_file_count == observed_video_frame_file_count,
        "generated materialization metadata video_frame_file_count does not match observed video frames: metadata={}, observed={}",
        metadata.video_frame_file_count,
        observed_video_frame_file_count
    );

    Ok(metadata)
}

fn expected_video_frames_per_scene(config: &ShapeFlowConfig) -> Result<usize> {
    let n_motion_slots = usize::try_from(config.scene.n_motion_slots).context(
        "scene.n_motion_slots does not fit into usize when deriving expected video frames",
    )?;
    Ok(n_motion_slots * usize::from(config.scene.event_duration_frames))
}

fn expected_scene_ids(scene_count: u32) -> Vec<String> {
    (0..scene_count)
        .map(|scene_index| canonical_scene_id(u64::from(scene_index)))
        .collect()
}

fn expected_artifact_filenames(scene_ids: &[String], extension: &str) -> BTreeSet<String> {
    scene_ids
        .iter()
        .map(|scene_id| format!("{scene_id}.{extension}"))
        .collect()
}

fn expected_target_filenames(
    scene_ids: &[String],
    effective_scene_configs: &[ShapeFlowConfig],
) -> BTreeSet<String> {
    scene_ids
        .iter()
        .enumerate()
        .flat_map(|(scene_index, scene_id)| {
            expected_target_task_ids(usize::from(
                effective_scene_configs[scene_index].scene.n_shapes,
            ))
            .into_iter()
            .map(move |task_id| format!("{scene_id}_{task_id}.sft"))
        })
        .collect()
}

fn expected_video_frame_filenames(frame_count: usize) -> BTreeSet<String> {
    (0..frame_count)
        .map(|frame_index| format!("frame_{frame_index:06}.png"))
        .collect()
}

fn collect_directory_entry_names(directory: &Utf8Path) -> Result<BTreeSet<String>> {
    let entries = std::fs::read_dir(directory.as_std_path()).with_context(|| {
        format!(
            "failed to read generated artifact directory {}",
            directory.as_str()
        )
    })?;
    let mut filenames = BTreeSet::new();
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "failed to iterate generated artifact directory {}",
                directory.as_str()
            )
        })?;
        filenames.insert(entry.file_name().to_string_lossy().to_string());
    }
    Ok(filenames)
}

fn ensure_exact_file_set(
    directory: &Utf8Path,
    expected: &BTreeSet<String>,
    context_label: &str,
) -> Result<BTreeSet<String>> {
    let observed = collect_directory_entry_names(directory)?;
    let missing: Vec<String> = expected.difference(&observed).cloned().collect();
    let unexpected: Vec<String> = observed.difference(expected).cloned().collect();

    ensure!(
        missing.is_empty(),
        "generated materialization metadata validation failed for {context_label}: missing entries under {}: {:?}",
        directory.as_str(),
        missing
    );
    ensure!(
        unexpected.is_empty(),
        "generated materialization metadata validation failed for {context_label}: unexpected entries under {}: {:?}",
        directory.as_str(),
        unexpected
    );
    Ok(observed)
}

fn validate_video_frame_file_sets(
    video_frames_dir: &Utf8Path,
    scene_ids: &[String],
    expected_frames_per_scene: &[usize],
) -> Result<usize> {
    let expected_scene_dirs: BTreeSet<String> = scene_ids.iter().cloned().collect();
    let observed_scene_dirs = ensure_exact_file_set(
        video_frames_dir,
        &expected_scene_dirs,
        "video_frames scene directories",
    )?;
    let mut observed_frame_file_count = 0usize;

    for (scene_index, scene_id) in scene_ids.iter().enumerate() {
        let expected_frame_files =
            expected_video_frame_filenames(expected_frames_per_scene[scene_index]);
        let scene_dir = video_frames_dir.join(scene_id);
        let observed_scene_frames = ensure_exact_file_set(
            &scene_dir,
            &expected_frame_files,
            &format!("video frames for scene_id={scene_id}"),
        )?;
        observed_frame_file_count += observed_scene_frames.len();
    }

    ensure!(
        observed_scene_dirs.len() == scene_ids.len(),
        "generated materialization validation failed for video_frames scene directories: scene_id count mismatch"
    );

    Ok(observed_frame_file_count)
}

fn validate_target_artifacts(
    targets_dir: &Utf8Path,
    effective_scene_configs: &[ShapeFlowConfig],
    scene_ids: &[String],
    samples_per_event: usize,
) -> Result<(usize, usize)> {
    let expected_target_filenames = expected_target_filenames(scene_ids, effective_scene_configs);
    let observed_target_files =
        ensure_exact_file_set(targets_dir, &expected_target_filenames, "target artifacts")?;

    let mut observed_target_file_count = 0usize;
    let mut observed_target_segment_count = 0usize;

    for (scene_index, scene_id) in scene_ids.iter().enumerate() {
        let effective_config = &effective_scene_configs[scene_index];
        let expected_task_ids =
            expected_target_task_ids(usize::from(effective_config.scene.n_shapes));
        let params = SceneGenerationParams {
            config: effective_config,
            scene_index: u64::try_from(scene_index).with_context(|| {
                format!(
                    "failed to convert scene index to u64 for target payload validation: scene_index={scene_index}"
                )
            })?,
            samples_per_event,
            projection: SceneProjectionMode::SoftQuadrants,
        };
        let output = generate_scene(&params).with_context(|| {
            format!(
                "failed to regenerate scene for target payload validation: scene_index={scene_index}"
            )
        })?;

        let generated_targets = generate_all_scene_targets(&output).with_context(|| {
            format!("failed to regenerate target payloads for scene_id={scene_id}")
        })?;
        let mut target_segments_by_task = BTreeMap::new();
        for target in generated_targets {
            let previous = target_segments_by_task.insert(target.task_id.clone(), target.segments);
            ensure!(
                previous.is_none(),
                "generated target payload validation failed for scene_id={scene_id}: duplicate task_id={}",
                target.task_id
            );
        }
        ensure!(
            target_segments_by_task.len() == expected_task_ids.len(),
            "generated target payload validation failed for scene_id={scene_id}: deterministic generation produced {} targets, expected {}",
            target_segments_by_task.len(),
            expected_task_ids.len()
        );

        for task_id in expected_task_ids {
            let expected_segments = target_segments_by_task.remove(&task_id).with_context(|| {
                format!(
                    "generated target payload validation failed for scene_id={scene_id}: deterministic target payload missing task_id={task_id}"
                )
            })?;
            let filename = format!("{scene_id}_{task_id}.sft");
            let path = targets_dir.join(filename);
            let bytes = std::fs::read(path.as_std_path()).with_context(|| {
                format!(
                    "failed to read generated target artifact for scene_id={scene_id}, task_id={task_id}"
                )
            })?;
            let artifact = deserialize_target_artifact(&bytes).with_context(|| {
                format!(
                    "failed to decode generated target artifact for scene_id={scene_id}, task_id={task_id}"
                )
            })?;
            ensure!(
                artifact.scene_id == *scene_id,
                "generated target payload validation failed for scene_id={scene_id}, task_id={task_id}: scene_id field mismatch: expected={scene_id}, observed={}",
                artifact.scene_id
            );
            ensure!(
                artifact.task_id == task_id,
                "generated target payload validation failed for scene_id={scene_id}, task_id={task_id}: task_id field mismatch: expected={task_id}, observed={}",
                artifact.task_id
            );
            ensure!(
                artifact.segments == expected_segments,
                "generated target payload validation failed for scene_id={scene_id}, task_id={task_id}: segment payload mismatch"
            );
            observed_target_file_count += 1;
            observed_target_segment_count += artifact.segments.len();
        }

        ensure!(
            target_segments_by_task.is_empty(),
            "generated target payload validation failed for scene_id={scene_id}: unexpected task ids: {:?}",
            target_segments_by_task.keys().collect::<Vec<_>>()
        );
    }

    ensure!(
        observed_target_files.len() == observed_target_file_count,
        "generated target payload validation failed: observed target file set count {} does not match validated target files {}",
        observed_target_files.len(),
        observed_target_file_count
    );

    Ok((observed_target_file_count, observed_target_segment_count))
}

fn validate_non_target_artifact_payloads(
    output_root: &Utf8Path,
    effective_scene_configs: &[ShapeFlowConfig],
    scene_ids: &[String],
    samples_per_event: usize,
) -> Result<()> {
    for (scene_index, scene_id) in scene_ids.iter().enumerate() {
        let effective_config = &effective_scene_configs[scene_index];
        let params = SceneGenerationParams {
            config: effective_config,
            scene_index: u64::try_from(scene_index).with_context(|| {
                format!(
                    "failed to convert scene index to u64 for generated artifact payload validation: scene_index={scene_index}"
                )
            })?,
            samples_per_event,
            projection: SceneProjectionMode::SoftQuadrants,
        };
        let output = generate_scene(&params).with_context(|| {
            format!(
                "failed to regenerate scene for generated artifact payload validation: scene_index={scene_index}"
            )
        })?;

        let expected_latent_artifact = LatentArtifact {
            schema_version: effective_config.schema_version,
            scene_id: scene_id.clone(),
            values: extract_latent_vector_from_scene(&output).with_context(|| {
                format!(
                    "failed to extract latent vector for generated artifact payload validation: scene_id={scene_id}"
                )
            })?,
        };
        let expected_latent_bytes = serialize_latent_artifact(&expected_latent_artifact)
            .with_context(|| {
                format!(
                    "failed to serialize expected latent artifact for generated artifact payload validation: scene_id={scene_id}"
                )
            })?;
        let latent_path = output_root.join("latent").join(format!("{scene_id}.bin"));
        let observed_latent_bytes =
            std::fs::read(latent_path.as_std_path()).with_context(|| {
                format!(
                    "failed to read generated latent artifact for payload validation: scene_id={scene_id}"
                )
            })?;
        ensure!(
            observed_latent_bytes == expected_latent_bytes,
            "generated latent artifact payload validation failed for scene_id={scene_id}: byte payload mismatch"
        );
        let observed_latent_artifact = deserialize_latent_artifact(&observed_latent_bytes)
            .with_context(|| {
            format!(
                "failed to decode generated latent artifact for payload validation: scene_id={scene_id}"
            )
        })?;
        ensure!(
            observed_latent_artifact == expected_latent_artifact,
            "generated latent artifact payload validation failed for scene_id={scene_id}: decoded payload mismatch"
        );

        let expected_tabular_rows = generate_tabular_motion_rows(&output).with_context(|| {
            format!(
                "failed to generate expected tabular rows for generated artifact payload validation: scene_id={scene_id}"
            )
        })?;
        let expected_tabular_csv = serialize_tabular_motion_rows_csv(&expected_tabular_rows);
        let tabular_path = output_root.join("tabular").join(format!("{scene_id}.csv"));
        let observed_tabular_csv =
            std::fs::read_to_string(tabular_path.as_std_path()).with_context(|| {
                format!(
                    "failed to read generated tabular artifact for payload validation: scene_id={scene_id}"
                )
            })?;
        ensure!(
            observed_tabular_csv == expected_tabular_csv,
            "generated tabular artifact payload validation failed for scene_id={scene_id}: csv payload mismatch"
        );

        let expected_text_lines =
            generate_scene_text_lines_with_scene_config(&output, &effective_config.scene)
                .with_context(|| {
                    format!(
                        "failed to generate expected text lines for generated artifact payload validation: scene_id={scene_id}"
                    )
                })?;
        let expected_text_body = serialize_scene_text(&expected_text_lines);
        let text_path = output_root.join("text").join(format!("{scene_id}.txt"));
        let observed_text_body =
            std::fs::read_to_string(text_path.as_std_path()).with_context(|| {
                format!(
                    "failed to read generated text artifact for payload validation: scene_id={scene_id}"
                )
            })?;
        ensure!(
            observed_text_body == expected_text_body,
            "generated text artifact payload validation failed for scene_id={scene_id}: text payload mismatch"
        );

        let expected_image_png =
            render_scene_image_png_with_scene_config(&output, &effective_config.scene)
                .with_context(|| {
                format!(
                    "failed to render expected image artifact for generated artifact payload validation: scene_id={scene_id}"
                )
            })?;
        let image_path = output_root.join("image").join(format!("{scene_id}.png"));
        let observed_image_png = std::fs::read(image_path.as_std_path()).with_context(|| {
            format!(
                "failed to read generated image artifact for payload validation: scene_id={scene_id}"
            )
        })?;
        ensure!(
            observed_image_png == expected_image_png,
            "generated image artifact payload validation failed for scene_id={scene_id}: png payload mismatch"
        );

        let expected_sound_wav = render_scene_sound_wav(
            &output,
            effective_config.scene.sound_sample_rate_hz,
            effective_config.scene.sound_frames_per_second,
            effective_config.scene.sound_modulation_depth_per_mille,
            effective_config.scene.sound_channel_mapping,
        )
        .with_context(|| {
            format!(
                "failed to render expected sound artifact for generated artifact payload validation: scene_id={scene_id}"
            )
        })?;
        let sound_path = output_root.join("sound").join(format!("{scene_id}.wav"));
        let observed_sound_wav = std::fs::read(sound_path.as_std_path()).with_context(|| {
            format!(
                "failed to read generated sound artifact for payload validation: scene_id={scene_id}"
            )
        })?;
        ensure!(
            observed_sound_wav == expected_sound_wav,
            "generated sound artifact payload validation failed for scene_id={scene_id}: wav payload mismatch"
        );

        let expected_video_frames = render_scene_video_frames_png_with_keyframe_border(
            &output,
            effective_config.scene.resolution,
            effective_config.scene.video_keyframe_border,
        )
        .with_context(|| {
            format!(
                "failed to render expected video frames for generated artifact payload validation: scene_id={scene_id}"
            )
        })?;
        let video_frame_dir = output_root.join("video_frames").join(scene_id);
        for (frame_index, expected_frame_png) in expected_video_frames.iter().enumerate() {
            let frame_path = video_frame_dir.join(format!("frame_{frame_index:06}.png"));
            let observed_frame_png = std::fs::read(frame_path.as_std_path()).with_context(|| {
                format!(
                    "failed to read generated video frame for payload validation: scene_id={scene_id}, frame_index={frame_index}"
                )
            })?;
            ensure!(
                observed_frame_png == expected_frame_png.as_slice(),
                "generated video frame payload validation failed for scene_id={scene_id}, frame_index={frame_index}: png payload mismatch"
            );
        }
    }
    Ok(())
}
