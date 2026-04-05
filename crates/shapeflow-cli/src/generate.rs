use crate::load_config;
use crate::materialization_metadata::MaterializationMetadataRecord;
use crate::site_metadata::SiteMetadataRecord;
use crate::split_assignments_metadata::SplitAssignmentsMetadataRecord;
use anyhow::{Context, Result, ensure};
use camino::Utf8PathBuf;
use rayon::prelude::*;
use shapeflow_core::{
    LatentArtifact, SceneGenerationParams, SceneProjectionMode, ShapeFlowConfig, TargetArtifact,
    build_split_assignments, canonical_scene_id, deserialize_latent_artifact,
    extract_latent_vector_from_scene, generate_all_scene_targets, generate_scene,
    generate_scene_text_lines_with_scene_config, generate_tabular_motion_rows,
    render_scene_image_png_with_scene_config, render_scene_sound_wav,
    render_scene_video_frames_png_with_keyframe_border, serialize_latent_artifact,
    serialize_scene_text, serialize_site_graph_artifact, serialize_tabular_motion_rows_csv,
    serialize_target_artifact, validate_site_graph_with_artifact,
};

#[derive(Debug)]
struct SceneMaterialization {
    scene_index: u32,
    scene_id: String,
    latent_bytes: Vec<u8>,
    tabular_csv: String,
    text_body: String,
    image_png_bytes: Vec<u8>,
    sound_wav_bytes: Vec<u8>,
    video_frame_pngs: Vec<Vec<u8>>,
    target_artifacts: Vec<(String, Vec<u8>, usize)>,
}

fn build_scene_materialization(
    config: &ShapeFlowConfig,
    scene_index: u32,
    samples_per_event: usize,
) -> Result<SceneMaterialization> {
    let params = SceneGenerationParams {
        config,
        scene_index: u64::from(scene_index),
        samples_per_event,
        projection: SceneProjectionMode::SoftQuadrants,
    };
    let output = generate_scene(&params)
        .with_context(|| format!("scene generation failed for scene_index={scene_index}"))?;
    let scene_id = canonical_scene_id(u64::from(scene_index));
    let latent_values = extract_latent_vector_from_scene(&output)
        .with_context(|| format!("latent extraction failed for scene_index={scene_index}"))?;
    let latent_artifact = LatentArtifact {
        schema_version: config.schema_version,
        scene_id: scene_id.clone(),
        values: latent_values,
    };
    let latent_bytes = serialize_latent_artifact(&latent_artifact).with_context(|| {
        format!("latent artifact serialization failed for scene_index={scene_index}")
    })?;
    let decoded_latent = deserialize_latent_artifact(&latent_bytes).with_context(|| {
        format!("latent artifact deserialization failed for scene_index={scene_index}")
    })?;
    ensure!(
        decoded_latent == latent_artifact,
        "latent artifact roundtrip mismatch after serialization for scene_index={scene_index}"
    );

    let tabular_rows = generate_tabular_motion_rows(&output)
        .with_context(|| format!("tabular row generation failed for scene_index={scene_index}"))?;
    let tabular_csv = serialize_tabular_motion_rows_csv(&tabular_rows);
    let text_lines = generate_scene_text_lines_with_scene_config(&output, &config.scene)
        .with_context(|| format!("text generation failed for scene_index={scene_index}"))?;
    let text_body = serialize_scene_text(&text_lines);
    let image_png_bytes = render_scene_image_png_with_scene_config(&output, &config.scene)
        .with_context(|| format!("image rendering failed for scene_index={scene_index}"))?;
    let video_frame_pngs = render_scene_video_frames_png_with_keyframe_border(
        &output,
        config.scene.resolution,
        config.scene.video_keyframe_border,
    )
    .with_context(|| format!("video frame rendering failed for scene_index={scene_index}"))?;
    let sound_wav_bytes = render_scene_sound_wav(
        &output,
        config.scene.sound_sample_rate_hz,
        config.scene.sound_frames_per_second,
        config.scene.sound_modulation_depth_per_mille,
        config.scene.sound_channel_mapping,
    )
    .with_context(|| format!("sound rendering failed for scene_index={scene_index}"))?;

    let mut targets = generate_all_scene_targets(&output)
        .with_context(|| format!("target generation failed for scene_index={scene_index}"))?;
    targets.sort_by(|left, right| left.task_id.cmp(&right.task_id));
    let target_artifacts = targets
        .into_iter()
        .map(|target| {
            let task_id = target.task_id;
            let artifact = TargetArtifact {
                schema_version: config.schema_version,
                scene_id: scene_id.clone(),
                task_id: task_id.clone(),
                segments: target.segments,
            };
            let bytes = serialize_target_artifact(&artifact).with_context(|| {
                format!("target artifact serialization failed for scene_index={scene_index}, task_id={task_id}")
            })?;
            Ok((task_id, bytes, artifact.segments.len()))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(SceneMaterialization {
        scene_index,
        scene_id,
        latent_bytes,
        tabular_csv,
        text_body,
        image_png_bytes,
        sound_wav_bytes,
        video_frame_pngs,
        target_artifacts,
    })
}

#[allow(dead_code)]
pub(crate) fn run_generate(
    config_path: Utf8PathBuf,
    output_dir: Utf8PathBuf,
    scene_count: u32,
    samples_per_event: usize,
) -> Result<()> {
    ensure!(scene_count > 0, "scene_count must be > 0");
    ensure!(samples_per_event > 0, "samples_per_event must be > 0");

    let config = load_config(config_path)?;
    config.validate()?;
    let dataset_identity = config.dataset_identity()?;

    let metadata_dir = output_dir.join("metadata");
    let targets_dir = output_dir.join("targets");
    let latent_dir = output_dir.join("latent");
    let tabular_dir = output_dir.join("tabular");
    let text_dir = output_dir.join("text");
    let image_dir = output_dir.join("image");
    let video_frames_dir = output_dir.join("video_frames");
    let sound_dir = output_dir.join("sound");
    std::fs::create_dir_all(metadata_dir.as_std_path())
        .with_context(|| format!("failed to create metadata dir {}", metadata_dir))?;
    std::fs::create_dir_all(targets_dir.as_std_path())
        .with_context(|| format!("failed to create targets dir {}", targets_dir))?;
    std::fs::create_dir_all(latent_dir.as_std_path())
        .with_context(|| format!("failed to create latent dir {}", latent_dir))?;
    std::fs::create_dir_all(tabular_dir.as_std_path())
        .with_context(|| format!("failed to create tabular dir {}", tabular_dir))?;
    std::fs::create_dir_all(text_dir.as_std_path())
        .with_context(|| format!("failed to create text dir {}", text_dir))?;
    std::fs::create_dir_all(image_dir.as_std_path())
        .with_context(|| format!("failed to create image dir {}", image_dir))?;
    std::fs::create_dir_all(video_frames_dir.as_std_path())
        .with_context(|| format!("failed to create video_frames dir {}", video_frames_dir))?;
    std::fs::create_dir_all(sound_dir.as_std_path())
        .with_context(|| format!("failed to create sound dir {}", sound_dir))?;

    let config_toml = toml::to_string_pretty(&config).context("failed to encode config TOML")?;
    std::fs::write(metadata_dir.join("config.toml").as_std_path(), config_toml)
        .context("failed to write metadata/config.toml")?;

    let (site_report, site_artifact) =
        validate_site_graph_with_artifact(&config).context("site graph validation failed")?;
    let site_graph_bytes = serialize_site_graph_artifact(&site_artifact)
        .context("site graph artifact serialization failed")?;
    std::fs::write(
        metadata_dir.join("site_graph.sfg").as_std_path(),
        site_graph_bytes,
    )
    .context("failed to write metadata/site_graph.sfg")?;

    let site_metadata = SiteMetadataRecord {
        master_seed: dataset_identity.master_seed,
        config_hash: dataset_identity.config_hash_hex.clone(),
        schema_version: config.schema_version,
        scene_count: site_report.scene_count,
        site_k: site_report.site_k,
        effective_k: site_report.effective_k,
        undirected_edge_count: site_report.undirected_edge_count,
        connected_components: site_report.connected_components,
        min_degree: site_report.min_degree,
        max_degree: site_report.max_degree,
        mean_degree: site_report.mean_degree,
        lambda2_estimate: site_report.lambda2_estimate,
    };
    let site_metadata_toml =
        toml::to_string_pretty(&site_metadata).context("failed to encode site metadata TOML")?;
    std::fs::write(
        metadata_dir.join("site_metadata.toml").as_std_path(),
        site_metadata_toml,
    )
    .context("failed to write metadata/site_metadata.toml")?;

    let scene_count_usize = scene_count as usize;
    let split_assignment_result =
        build_split_assignments(scene_count_usize).with_context(|| {
            format!("failed to build split assignments for scene_count={scene_count}")
        })?;

    let split_metadata = SplitAssignmentsMetadataRecord {
        master_seed: dataset_identity.master_seed,
        config_hash: dataset_identity.config_hash_hex.clone(),
        schema_version: config.schema_version,
        summary: split_assignment_result.summary.clone(),
        assignments: split_assignment_result.assignments.clone(),
    };
    let split_metadata_toml = toml::to_string_pretty(&split_metadata)
        .context("failed to encode split assignments TOML")?;
    std::fs::write(
        metadata_dir.join("split_assignments.toml").as_std_path(),
        split_metadata_toml,
    )
    .context("failed to write metadata/split_assignments.toml")?;

    let mut target_file_count = 0usize;
    let mut total_target_segments = 0usize;
    let mut latent_artifact_count = 0usize;
    let mut sound_file_count = 0usize;
    let mut tabular_file_count = 0usize;
    let mut text_file_count = 0usize;
    let mut image_file_count = 0usize;
    let mut video_frame_file_count = 0usize;
    let scene_materializations: Vec<SceneMaterialization> = rayon::ThreadPoolBuilder::new()
        .num_threads(config.parallelism.num_threads as usize)
        .build()
        .context("failed to build rayon thread pool")?
        .install(|| {
            (0..scene_count)
                .into_par_iter()
                .map(|scene_index| {
                    build_scene_materialization(&config, scene_index, samples_per_event)
                })
                .collect::<Result<Vec<_>>>()
        })?;

    let mut scene_materializations = scene_materializations;
    scene_materializations.sort_by_key(|scene| scene.scene_index);

    for materialization in scene_materializations {
        let scene_id = materialization.scene_id;
        let scene_index = materialization.scene_index;

        std::fs::write(
            latent_dir.join(format!("{scene_id}.bin")).as_std_path(),
            materialization.latent_bytes,
        )
        .with_context(|| {
            format!("failed to write latent artifact for scene_index={scene_index}")
        })?;
        latent_artifact_count += 1;

        std::fs::write(
            tabular_dir.join(format!("{scene_id}.csv")).as_std_path(),
            materialization.tabular_csv,
        )
        .with_context(|| {
            format!("failed to write tabular artifact for scene_index={scene_index}")
        })?;
        tabular_file_count += 1;
        std::fs::write(
            text_dir.join(format!("{scene_id}.txt")).as_std_path(),
            materialization.text_body,
        )
        .with_context(|| format!("failed to write text artifact for scene_index={scene_index}"))?;
        text_file_count += 1;
        std::fs::write(
            image_dir.join(format!("{scene_id}.png")).as_std_path(),
            materialization.image_png_bytes,
        )
        .with_context(|| format!("failed to write image artifact for scene_index={scene_index}"))?;
        image_file_count += 1;
        std::fs::write(
            sound_dir.join(format!("{scene_id}.wav")).as_std_path(),
            materialization.sound_wav_bytes,
        )
        .with_context(|| format!("failed to write sound artifact for scene_index={scene_index}"))?;
        sound_file_count += 1;
        let scene_video_frames_dir = video_frames_dir.join(&scene_id);
        std::fs::create_dir_all(scene_video_frames_dir.as_std_path()).with_context(|| {
            format!("failed to create video frame dir for scene_index={scene_index}")
        })?;
        for (frame_index, frame_png) in materialization.video_frame_pngs.into_iter().enumerate() {
            std::fs::write(
                scene_video_frames_dir
                    .join(format!("frame_{frame_index:06}.png"))
                    .as_std_path(),
                frame_png,
            )
            .with_context(|| {
                format!(
                    "failed to write video frame for scene_index={scene_index}, frame_index={frame_index}"
                )
            })?;
            video_frame_file_count += 1;
        }

        for (task_id, bytes, segment_count) in materialization.target_artifacts {
            total_target_segments += segment_count;
            let filename = format!("{scene_id}_{task_id}.sft");
            std::fs::write(targets_dir.join(filename).as_std_path(), bytes).with_context(|| {
                format!(
                    "failed to write target artifact for scene_index={scene_index}, task_id={task_id}"
                )
            })?;
            target_file_count += 1;
        }
    }

    let materialization_metadata = MaterializationMetadataRecord {
        master_seed: dataset_identity.master_seed,
        config_hash: dataset_identity.config_hash_hex.clone(),
        schema_version: config.schema_version,
        scene_count,
        samples_per_event,
        target_file_count,
        total_target_segments,
        latent_artifact_count,
        sound_file_count,
        tabular_file_count,
        text_file_count,
        image_file_count,
        video_frame_file_count,
    };
    let materialization_metadata_toml = toml::to_string_pretty(&materialization_metadata)
        .context("failed to encode materialization metadata TOML")?;
    std::fs::write(
        metadata_dir.join("materialization.toml").as_std_path(),
        materialization_metadata_toml,
    )
    .context("failed to write metadata/materialization.toml")?;

    println!("generation=ok");
    println!("output={output_dir}");
    println!(
        "scene_count={}, samples_per_event={}, target_file_count={}, sound_file_count={}, total_target_segments={}, tabular_file_count={}, text_file_count={}, image_file_count={}, video_frame_file_count={}",
        scene_count,
        samples_per_event,
        target_file_count,
        sound_file_count,
        total_target_segments,
        tabular_file_count,
        text_file_count,
        image_file_count,
        video_frame_file_count
    );
    println!("config_hash={}", dataset_identity.config_hash_hex);

    Ok(())
}
