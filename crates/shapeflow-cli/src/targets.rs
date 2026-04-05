use anyhow::{Context, Result, bail, ensure};
use shapeflow_core::{
    GeneratedTarget, ShapeFlowConfig, canonical_scene_id, generate_scene_targets_for_index,
    validate_generated_targets,
};

pub(crate) fn run_targets(
    config: &ShapeFlowConfig,
    scene_index: u32,
    samples_per_event: usize,
    task_selector: &str,
) -> Result<()> {
    ensure!(samples_per_event > 0, "samples_per_event must be > 0");

    let scene_index_u64 = u64::from(scene_index);
    let targets = generate_scene_targets_for_index(config, scene_index_u64, samples_per_event)
        .with_context(|| format!("target-only generation failed for scene_index={scene_index}"))?;

    let selected_targets = select_targets(&targets, task_selector)?;
    let report = validate_generated_targets(&selected_targets).with_context(|| {
        format!(
            "target validation failed for scene_index={scene_index}, task_selector={task_selector}"
        )
    })?;

    println!("targets=ok");
    println!(
        "scene_id={}, scene_index={}, samples_per_event={}, task_selector={}",
        canonical_scene_id(scene_index_u64),
        scene_index,
        samples_per_event,
        task_selector
    );
    println!(
        "target_count={}, total_target_segments={}, total_target_values={}",
        report.target_count, report.total_segments, report.total_values
    );
    for target in &selected_targets {
        println!(
            "task_id={},segment_count={}",
            target.task_id,
            target.segments.len()
        );
        for (segment_index, segment) in target.segments.iter().enumerate() {
            let values = segment
                .iter()
                .map(|value| format!("{value:.17}"))
                .collect::<Vec<_>>()
                .join(",");
            println!("segment={segment_index},values={values}");
        }
    }

    Ok(())
}

fn select_targets(
    targets: &[GeneratedTarget],
    task_selector: &str,
) -> Result<Vec<GeneratedTarget>> {
    if task_selector == "all" {
        return Ok(targets.to_vec());
    }

    let selected = targets
        .iter()
        .filter(|target| {
            target.task_id == task_selector || target.task_id.starts_with(task_selector)
        })
        .cloned()
        .collect::<Vec<_>>();

    if selected.is_empty() {
        bail!(
            "unsupported task selector '{task_selector}'. use 'all', a task prefix like 'oqp', or an exact task id"
        );
    }

    Ok(selected)
}
