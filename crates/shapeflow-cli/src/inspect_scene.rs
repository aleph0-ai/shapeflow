use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use shapeflow_core::{
    SceneGenerationParams, SceneProjectionMode, ShapeFlowConfig, canonical_scene_id,
    generate_all_scene_targets, generate_scene, shape_identity_for_index,
    validate_generated_targets,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShapeInspectSummary {
    shape_index: usize,
    shape_id: String,
    trajectory_point_count: usize,
    soft_membership_count: usize,
    event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneInspectReport {
    scene_index: u64,
    scene_id: String,
    samples_per_event: usize,
    allow_simultaneous: bool,
    motion_event_count: usize,
    time_slot_count: usize,
    simultaneous_slot_count: usize,
    max_shapes_per_slot: usize,
    schedule_scene_layout: u64,
    schedule_trajectory: u64,
    schedule_text_grammar: u64,
    schedule_lexical_noise: u64,
    shape_summaries: Vec<ShapeInspectSummary>,
    target_count: usize,
    total_target_segments: usize,
    total_target_values: usize,
    target_segment_counts: Vec<(String, usize)>,
}

pub(crate) fn run_inspect_scene(
    config: &ShapeFlowConfig,
    scene_index: u32,
    samples_per_event: usize,
) -> Result<()> {
    ensure!(samples_per_event > 0, "samples_per_event must be > 0");

    let report = build_scene_inspection_report(config, scene_index, samples_per_event)?;
    print_scene_inspection_report(&report);
    Ok(())
}

fn build_scene_inspection_report(
    config: &ShapeFlowConfig,
    scene_index: u32,
    samples_per_event: usize,
) -> Result<SceneInspectReport> {
    let scene_index_u64 = u64::from(scene_index);
    let params = SceneGenerationParams {
        config,
        scene_index: scene_index_u64,
        samples_per_event,
        projection: SceneProjectionMode::SoftQuadrants,
    };
    let output = generate_scene(&params)
        .with_context(|| format!("scene generation failed for scene_index={scene_index}"))?;

    let mut event_count_per_shape = vec![0usize; output.shape_paths.len()];
    let mut shape_indexes_per_slot: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    for event in &output.motion_events {
        ensure!(
            event.shape_index < event_count_per_shape.len(),
            "generated scene emitted out-of-bounds shape index {} for scene with {} shapes",
            event.shape_index,
            event_count_per_shape.len()
        );
        event_count_per_shape[event.shape_index] += 1;
        shape_indexes_per_slot
            .entry(event.time_slot)
            .or_default()
            .insert(event.shape_index);
    }

    let time_slot_count = shape_indexes_per_slot.len();
    let simultaneous_slot_count = shape_indexes_per_slot
        .values()
        .filter(|shape_indexes| shape_indexes.len() > 1)
        .count();
    let max_shapes_per_slot = shape_indexes_per_slot
        .values()
        .map(BTreeSet::len)
        .max()
        .unwrap_or(0);

    let mut shape_summaries = Vec::with_capacity(output.shape_paths.len());
    for shape_path in &output.shape_paths {
        let identity = shape_identity_for_index(shape_path.shape_index).with_context(|| {
            format!(
                "failed to map shape identity for shape_index={} in scene_index={scene_index}",
                shape_path.shape_index
            )
        })?;
        shape_summaries.push(ShapeInspectSummary {
            shape_index: shape_path.shape_index,
            shape_id: identity.shape_id,
            trajectory_point_count: shape_path.trajectory_points.len(),
            soft_membership_count: shape_path.soft_memberships.as_ref().map_or(0, Vec::len),
            event_count: event_count_per_shape[shape_path.shape_index],
        });
    }
    shape_summaries.sort_by_key(|summary| summary.shape_index);

    let mut targets = generate_all_scene_targets(&output)
        .with_context(|| format!("target generation failed for scene_index={scene_index}"))?;
    targets.sort_by(|left, right| left.task_id.cmp(&right.task_id));
    let target_validation = validate_generated_targets(&targets)
        .with_context(|| format!("target validation failed for scene_index={scene_index}"))?;
    let target_segment_counts = targets
        .iter()
        .map(|target| (target.task_id.clone(), target.segments.len()))
        .collect::<Vec<_>>();

    Ok(SceneInspectReport {
        scene_index: output.scene_index,
        scene_id: canonical_scene_id(output.scene_index),
        samples_per_event,
        allow_simultaneous: config.scene.allow_simultaneous,
        motion_event_count: output.motion_events.len(),
        time_slot_count,
        simultaneous_slot_count,
        max_shapes_per_slot,
        schedule_scene_layout: output.schedule.scene_layout,
        schedule_trajectory: output.schedule.trajectory,
        schedule_text_grammar: output.schedule.text_grammar,
        schedule_lexical_noise: output.schedule.lexical_noise,
        shape_summaries,
        target_count: target_validation.target_count,
        total_target_segments: target_validation.total_segments,
        total_target_values: target_validation.total_values,
        target_segment_counts,
    })
}

fn print_scene_inspection_report(report: &SceneInspectReport) {
    println!("inspect-scene=ok");
    println!(
        "scene_id={}, scene_index={}, samples_per_event={}, allow_simultaneous={}",
        report.scene_id, report.scene_index, report.samples_per_event, report.allow_simultaneous
    );
    println!(
        "shape_count={}, motion_event_count={}, time_slot_count={}, simultaneous_slot_count={}, max_shapes_per_slot={}",
        report.shape_summaries.len(),
        report.motion_event_count,
        report.time_slot_count,
        report.simultaneous_slot_count,
        report.max_shapes_per_slot
    );
    println!(
        "scene_layout_seed={}, trajectory_seed={}, text_grammar_seed={}, lexical_noise_seed={}",
        report.schedule_scene_layout,
        report.schedule_trajectory,
        report.schedule_text_grammar,
        report.schedule_lexical_noise
    );
    for shape in &report.shape_summaries {
        println!(
            "shape_index={}, shape_id={}, trajectory_points={}, soft_membership_points={}, motion_events={}",
            shape.shape_index,
            shape.shape_id,
            shape.trajectory_point_count,
            shape.soft_membership_count,
            shape.event_count
        );
    }

    let target_segments = report
        .target_segment_counts
        .iter()
        .map(|(task_id, segments)| format!("{task_id}:{segments}"))
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "target_count={}, total_target_segments={}, total_target_values={}, target_segments={}",
        report.target_count,
        report.total_target_segments,
        report.total_target_values,
        target_segments
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config should parse")
    }

    #[test]
    fn inspect_scene_report_smoke_bootstrap_config() {
        let config = bootstrap_config();
        config.validate().expect("bootstrap config should validate");

        let report = build_scene_inspection_report(&config, 0, 24)
            .expect("scene inspection should succeed for bootstrap config");

        assert_eq!(report.scene_index, 0);
        assert_eq!(report.scene_id, canonical_scene_id(0));
        assert_eq!(
            report.shape_summaries.len(),
            usize::from(config.scene.n_shapes)
        );
        assert_eq!(
            report.target_count,
            shapeflow_core::expected_target_task_ids(report.shape_summaries.len()).len(),
            "all expected target surfaces should exist"
        );
        assert!(
            report.total_target_segments >= report.target_count,
            "each target should contribute at least one segment"
        );
        assert!(
            report.max_shapes_per_slot >= 1,
            "at least one shape must be present in every occupied slot"
        );
    }

    #[test]
    fn inspect_scene_rejects_zero_samples_per_event() {
        let config = bootstrap_config();
        config.validate().expect("bootstrap config should validate");
        assert!(
            run_inspect_scene(&config, 0, 0).is_err(),
            "inspect-scene should reject samples_per_event=0"
        );
    }
}
