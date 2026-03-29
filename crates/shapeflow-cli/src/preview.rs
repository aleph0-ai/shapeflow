use anyhow::{Context, Result, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use shapeflow_core::{
    OrderedQuadrantPassageTarget, SceneGenerationParams, SceneProjectionMode, ShapeFlowConfig,
    canonical_scene_id, generate_ordered_quadrant_passage_targets, generate_scene,
    generate_scene_text_lines, generate_tabular_motion_rows, render_scene_image_png,
    render_scene_sound_wav, render_scene_video_frames_png, serialize_scene_text,
    serialize_tabular_motion_rows_csv, validate_ordered_quadrant_passage_targets,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewReport {
    scene_id: String,
    scene_index: u32,
    output_dir: Utf8PathBuf,
    samples_per_event: usize,
    target_count: usize,
    total_target_segments: usize,
    hard_target_segments: usize,
    video_frame_count: usize,
}

pub(crate) fn run_preview(
    config: &ShapeFlowConfig,
    output_root: &Utf8Path,
    scene_index: u32,
    samples_per_event: usize,
) -> Result<()> {
    ensure!(samples_per_event > 0, "samples_per_event must be > 0");

    let report = build_preview_artifacts(config, output_root, scene_index, samples_per_event)?;
    print_preview_report(&report);
    Ok(())
}

fn build_preview_artifacts(
    config: &ShapeFlowConfig,
    output_root: &Utf8Path,
    scene_index: u32,
    samples_per_event: usize,
) -> Result<PreviewReport> {
    let scene_index_u64 = u64::from(scene_index);
    let params = SceneGenerationParams {
        config,
        scene_index: scene_index_u64,
        samples_per_event,
        projection: SceneProjectionMode::SoftQuadrants,
    };
    let output = generate_scene(&params)
        .with_context(|| format!("scene generation failed for scene_index={scene_index}"))?;

    let scene_id = canonical_scene_id(scene_index_u64);
    let scene_output_dir = output_root.join(&scene_id);
    let video_frames_dir = scene_output_dir.join("video_frames");
    std::fs::create_dir_all(video_frames_dir.as_std_path()).with_context(|| {
        format!(
            "failed to create preview output directory {}",
            video_frames_dir.as_str()
        )
    })?;

    let tabular_rows = generate_tabular_motion_rows(&output)
        .with_context(|| format!("tabular row generation failed for scene_index={scene_index}"))?;
    let tabular_csv = serialize_tabular_motion_rows_csv(&tabular_rows);
    std::fs::write(
        scene_output_dir.join("tabular.csv").as_std_path(),
        tabular_csv.as_bytes(),
    )
    .with_context(|| format!("failed to write tabular preview for scene_id={scene_id}"))?;

    let text_lines = generate_scene_text_lines(&output)
        .with_context(|| format!("text generation failed for scene_index={scene_index}"))?;
    let text_body = serialize_scene_text(&text_lines);
    std::fs::write(
        scene_output_dir.join("text.txt").as_std_path(),
        text_body.as_bytes(),
    )
    .with_context(|| format!("failed to write text preview for scene_id={scene_id}"))?;

    let image_png = render_scene_image_png(&output, config.scene.resolution)
        .with_context(|| format!("image rendering failed for scene_index={scene_index}"))?;
    std::fs::write(scene_output_dir.join("image.png").as_std_path(), image_png)
        .with_context(|| format!("failed to write image preview for scene_id={scene_id}"))?;

    let sound_wav = render_scene_sound_wav(
        &output,
        config.scene.sound_sample_rate_hz,
        config.scene.sound_frames_per_second,
        config.scene.sound_modulation_depth_per_mille,
        config.scene.sound_channel_mapping,
    )
    .with_context(|| format!("sound rendering failed for scene_index={scene_index}"))?;
    std::fs::write(scene_output_dir.join("sound.wav").as_std_path(), sound_wav)
        .with_context(|| format!("failed to write sound preview for scene_id={scene_id}"))?;

    let video_frames = render_scene_video_frames_png(&output, config.scene.resolution)
        .with_context(|| format!("video frame rendering failed for scene_index={scene_index}"))?;
    for (frame_index, frame_png) in video_frames.iter().enumerate() {
        let frame_name = format!("frame_{frame_index:06}.png");
        std::fs::write(video_frames_dir.join(frame_name).as_std_path(), frame_png).with_context(
            || {
                format!(
                    "failed to write video-frame preview for scene_id={scene_id}, frame_index={frame_index}"
                )
            },
        )?;
    }

    let mut targets = generate_ordered_quadrant_passage_targets(&output)
        .with_context(|| format!("target generation failed for scene_index={scene_index}"))?;
    targets.sort_by_key(|target| target.shape_index);
    let target_validation = validate_ordered_quadrant_passage_targets(&targets)
        .with_context(|| format!("target validation failed for scene_index={scene_index}"))?;
    let targets_preview_text = render_targets_preview_text(&targets);
    std::fs::write(
        scene_output_dir.join("targets_oqp.txt").as_std_path(),
        targets_preview_text.as_bytes(),
    )
    .with_context(|| format!("failed to write target preview for scene_id={scene_id}"))?;

    Ok(PreviewReport {
        scene_id,
        scene_index,
        output_dir: scene_output_dir,
        samples_per_event,
        target_count: target_validation.shape_target_count,
        total_target_segments: target_validation.total_segments,
        hard_target_segments: target_validation.hard_segment_count,
        video_frame_count: video_frames.len(),
    })
}

fn render_targets_preview_text(targets: &[OrderedQuadrantPassageTarget]) -> String {
    let mut lines = Vec::new();
    for target in targets {
        let task_id = format!("oqp{:04}", target.shape_index);
        lines.push(format!(
            "task_id={task_id},shape_index={},segment_count={}",
            target.shape_index,
            target.segments.len()
        ));
        for (segment_index, segment) in target.segments.iter().enumerate() {
            let components = segment.as_array();
            lines.push(format!(
                "segment={segment_index},q1={:.6},q2={:.6},q3={:.6},q4={:.6}",
                components[0], components[1], components[2], components[3]
            ));
        }
    }
    lines.join("\n")
}

fn print_preview_report(report: &PreviewReport) {
    println!("preview=ok");
    println!("output={}", report.output_dir.as_str());
    println!(
        "scene_id={}, scene_index={}, samples_per_event={}",
        report.scene_id, report.scene_index, report.samples_per_event
    );
    println!(
        "target_count={}, total_target_segments={}, hard_target_segments={}, video_frame_count={}",
        report.target_count,
        report.total_target_segments,
        report.hard_target_segments,
        report.video_frame_count
    );
    println!(
        "artifacts=text.txt,tabular.csv,image.png,sound.wav,targets_oqp.txt,video_frames/frame_XXXXXX.png"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::path::Path;

    fn bootstrap_config_path() -> Utf8PathBuf {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        Utf8PathBuf::from_path_buf(
            manifest_dir
                .join("../../configs/bootstrap.toml")
                .to_path_buf(),
        )
        .expect("bootstrap config path should be utf-8")
    }

    #[test]
    fn preview_smoke_bootstrap_config() {
        let config_path = bootstrap_config_path();
        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let output_std = std::env::temp_dir().join(format!("shapeflow-preview-smoke-{nanos}"));
        let output_path =
            Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp path should be utf-8");

        run_preview(&config, output_path.as_ref(), 0, 24)
            .expect("preview should succeed for bootstrap config");

        let scene_dir = output_path.join(canonical_scene_id(0));
        assert!(
            scene_dir.join("text.txt").as_std_path().exists(),
            "preview should write text.txt"
        );
        assert!(
            scene_dir.join("tabular.csv").as_std_path().exists(),
            "preview should write tabular.csv"
        );
        assert!(
            scene_dir.join("image.png").as_std_path().exists(),
            "preview should write image.png"
        );
        assert!(
            scene_dir.join("sound.wav").as_std_path().exists(),
            "preview should write sound.wav"
        );
        assert!(
            scene_dir.join("targets_oqp.txt").as_std_path().exists(),
            "preview should write targets_oqp.txt"
        );
        let video_frames_dir = scene_dir.join("video_frames");
        assert!(
            video_frames_dir.as_std_path().exists(),
            "preview should create video_frames directory"
        );
        let frame_count = std::fs::read_dir(video_frames_dir.as_std_path())
            .expect("video_frames dir should be readable")
            .filter_map(|entry| entry.ok())
            .count();
        assert!(frame_count > 0, "preview should render at least one frame");

        std::fs::remove_dir_all(output_std).expect("temp output should be removable");
    }

    #[test]
    fn preview_rejects_zero_samples_per_event() {
        let config_path = bootstrap_config_path();
        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let output_std =
            std::env::temp_dir().join(format!("shapeflow-preview-zero-samples-{nanos}"));
        let output_path =
            Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp path should be utf-8");

        let result = run_preview(&config, output_path.as_ref(), 0, 0);
        assert!(result.is_err(), "preview should reject samples_per_event=0");
        if output_std.exists() {
            std::fs::remove_dir_all(output_std).expect("temp output should be removable");
        }
    }
}
