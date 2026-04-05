use crate::config::{ConfigError, EasingFamily, SceneConfig, ShapeFlowConfig};
use crate::landscape::{LandscapeError, SoftQuadrantMembership, positional_identity};
use crate::seed_schedule::SceneSeedSchedule;
use crate::trajectory::{
    NormalizedPoint, TrajectoryError, sample_random_linear_path_points_with_complexity,
};
use rand::RngCore;
use rand_chacha::ChaCha8Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneProjectionMode {
    TrajectoryOnly,
    SoftQuadrants,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneGenerationParams<'a> {
    pub config: &'a ShapeFlowConfig,
    pub scene_index: u64,
    pub samples_per_event: usize,
    pub projection: SceneProjectionMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneShapePath {
    pub shape_index: usize,
    pub trajectory_points: Vec<NormalizedPoint>,
    pub soft_memberships: Option<Vec<SoftQuadrantMembership>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionEvent {
    pub global_event_index: u32,
    pub time_slot: u32,
    pub shape_index: usize,
    pub shape_event_index: u16,
    pub start_point: NormalizedPoint,
    pub end_point: NormalizedPoint,
    pub duration_frames: u16,
    pub easing: EasingFamily,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionEventAccounting {
    pub expected_total: u32,
    pub expected_slots: u32,
    pub generated_total: u32,
    pub expected_per_shape: Vec<u16>,
    pub generated_per_shape: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneGenerationOutput {
    pub scene_index: u64,
    pub schedule: SceneSeedSchedule,
    pub shape_identity_assignment: crate::config::ShapeIdentityAssignment,
    pub shape_paths: Vec<SceneShapePath>,
    pub motion_events: Vec<MotionEvent>,
    pub accounting: MotionEventAccounting,
}

#[derive(Debug, thiserror::Error)]
pub enum SceneGenerationError {
    #[error("config validation failed: {0}")]
    Config(#[from] ConfigError),
    #[error("trajectory sampling failed: {0}")]
    Trajectory(#[from] TrajectoryError),
    #[error("landscape projection failed: {0}")]
    Landscape(#[from] LandscapeError),
    #[error("samples_per_event must be > 0, got {samples_per_event}")]
    InvalidSamplesPerEvent { samples_per_event: usize },
    #[error(
        "generated per-shape event count mismatch for shape {shape_index}: generated {generated}, expected {expected}"
    )]
    PerShapeEventCountMismatch {
        shape_index: usize,
        generated: u16,
        expected: u16,
    },
    #[error("internal index overflow while converting {field}: {value}")]
    IndexOverflow { field: &'static str, value: usize },
    #[error(
        "internal trajectory index out of bounds for shape {shape_index}: index {point_index}, len {len}"
    )]
    TrajectoryPointIndexOutOfBounds {
        shape_index: usize,
        point_index: usize,
        len: usize,
    },
    #[error("internal shape index out of bounds in generated event list: {shape_index}")]
    EventShapeIndexOutOfBounds { shape_index: usize },
    #[error("internal generated per-shape count overflow for shape {shape_index}")]
    GeneratedPerShapeCountOverflow { shape_index: usize },
    #[error(
        "randomized per-shape event allocation cannot represent event budget={budget} across n_shapes={n_shapes} with u16 per-shape counters"
    )]
    RandomizedEventAllocationOverflow { budget: u32, n_shapes: usize },
    #[error(
        "randomized event allocation failed while filling random ranges: remaining_events={remaining}, remaining_capacity={capacity}"
    )]
    RandomizedEventAllocationExhausted { remaining: u32, capacity: u32 },
}

pub fn generate_scene(
    params: &SceneGenerationParams<'_>,
) -> Result<SceneGenerationOutput, SceneGenerationError> {
    params.config.validate()?;
    if params.samples_per_event == 0 {
        return Err(SceneGenerationError::InvalidSamplesPerEvent {
            samples_per_event: 0,
        });
    }

    let schedule = SceneSeedSchedule::derive(params.config.master_seed, params.scene_index);
    let mut trajectory_rng = schedule.trajectory_rng();
    let mut event_index_rng = schedule.scene_layout_rng();
    let motion_events_per_shape =
        resolve_motion_events_per_shape(&params.config.scene, &mut event_index_rng)?;

    let mut shape_paths = Vec::with_capacity(motion_events_per_shape.len());
    for (shape_index, event_count) in motion_events_per_shape.iter().copied().enumerate() {
        let trajectory_points = if event_count == 0 {
            vec![NormalizedPoint::new(0.0, 0.0)?]
        } else {
            sample_random_linear_path_points_with_complexity(
                &mut trajectory_rng,
                usize::from(event_count),
                params.samples_per_event,
                params.config.scene.trajectory_complexity,
            )?
        };
        let soft_memberships = match params.projection {
            SceneProjectionMode::TrajectoryOnly => None,
            SceneProjectionMode::SoftQuadrants => Some(
                trajectory_points
                    .iter()
                    .map(|point| {
                        positional_identity(point.x, point.y, &params.config.positional_landscape)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        };

        shape_paths.push(SceneShapePath {
            shape_index,
            trajectory_points,
            soft_memberships,
        });
    }

    let motion_events = build_motion_events(
        &shape_paths,
        &params.config.scene,
        &motion_events_per_shape,
        params.samples_per_event,
        &mut event_index_rng,
    )?;
    let accounting = build_accounting(
        &motion_events,
        &params.config.scene,
        &motion_events_per_shape,
    )?;

    Ok(SceneGenerationOutput {
        scene_index: params.scene_index,
        schedule,
        shape_identity_assignment: params.config.scene.shape_identity_assignment,
        shape_paths,
        motion_events,
        accounting,
    })
}

fn build_motion_events(
    shape_paths: &[SceneShapePath],
    scene: &SceneConfig,
    motion_events_per_shape: &[u16],
    samples_per_event: usize,
    event_index_rng: &mut ChaCha8Rng,
) -> Result<Vec<MotionEvent>, SceneGenerationError> {
    let total_events = motion_events_per_shape
        .iter()
        .copied()
        .map(usize::from)
        .sum::<usize>();
    let mut motion_events = Vec::with_capacity(total_events);
    let n_motion_slots =
        usize::try_from(scene.n_motion_slots).map_err(|_| SceneGenerationError::IndexOverflow {
            field: "scene.n_motion_slots",
            value: usize::MAX,
        })?;

    if scene.allow_simultaneous {
        for shape_index in 0..shape_paths.len() {
            let events_for_shape = usize::from(motion_events_per_shape[shape_index]);
            let selected_slots =
                sample_distinct_sorted_slots(event_index_rng, n_motion_slots, events_for_shape)?;
            for (local_event_index, time_slot) in selected_slots.into_iter().enumerate() {
                motion_events.push(motion_event_from_shape_path(
                    &shape_paths[shape_index],
                    local_event_index,
                    samples_per_event,
                    to_u32(time_slot, "time_slot")?,
                    scene.event_duration_frames,
                    scene.easing_family,
                )?);
            }
        }
    } else {
        let selected_slots =
            sample_distinct_sorted_slots(event_index_rng, n_motion_slots, total_events)?;
        let mut slot_iter = selected_slots.into_iter();
        for shape_index in 0..shape_paths.len() {
            let events_for_shape = usize::from(motion_events_per_shape[shape_index]);
            for local_event_index in 0..events_for_shape {
                let time_slot = slot_iter
                    .next()
                    .expect("selected slot count must equal total events");
                motion_events.push(motion_event_from_shape_path(
                    &shape_paths[shape_index],
                    local_event_index,
                    samples_per_event,
                    to_u32(time_slot, "time_slot")?,
                    scene.event_duration_frames,
                    scene.easing_family,
                )?);
            }
        }
    }

    assign_random_global_event_indices(&mut motion_events, event_index_rng)?;

    Ok(motion_events)
}

fn sample_distinct_sorted_slots(
    rng: &mut ChaCha8Rng,
    n_slots: usize,
    count: usize,
) -> Result<Vec<usize>, SceneGenerationError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut slots: Vec<usize> = (0..n_slots).collect();
    for i in (1..slots.len()).rev() {
        let upper = u64::try_from(i + 1).expect("slot upper bound should fit u64");
        let j = usize::try_from(rng.next_u64() % upper).expect("slot index should fit usize");
        slots.swap(i, j);
    }
    if count > slots.len() {
        return Err(SceneGenerationError::IndexOverflow {
            field: "slot_count",
            value: count,
        });
    }
    slots.truncate(count);
    slots.sort_unstable();
    Ok(slots)
}

fn motion_event_from_shape_path(
    shape_path: &SceneShapePath,
    shape_event_index: usize,
    samples_per_event: usize,
    time_slot: u32,
    duration_frames: u16,
    easing: EasingFamily,
) -> Result<MotionEvent, SceneGenerationError> {
    let start_index = shape_event_index * samples_per_event;
    let end_index = (shape_event_index + 1) * samples_per_event;

    let start_point = shape_path
        .trajectory_points
        .get(start_index)
        .copied()
        .ok_or(SceneGenerationError::TrajectoryPointIndexOutOfBounds {
            shape_index: shape_path.shape_index,
            point_index: start_index,
            len: shape_path.trajectory_points.len(),
        })?;
    let end_point = shape_path.trajectory_points.get(end_index).copied().ok_or(
        SceneGenerationError::TrajectoryPointIndexOutOfBounds {
            shape_index: shape_path.shape_index,
            point_index: end_index,
            len: shape_path.trajectory_points.len(),
        },
    )?;

    Ok(MotionEvent {
        global_event_index: 0,
        time_slot,
        shape_index: shape_path.shape_index,
        shape_event_index: to_u16(shape_event_index, "shape_event_index")?,
        start_point,
        end_point,
        duration_frames,
        easing,
    })
}

fn assign_random_global_event_indices(
    motion_events: &mut [MotionEvent],
    rng: &mut ChaCha8Rng,
) -> Result<(), SceneGenerationError> {
    const MAX_RESHUFFLE_ATTEMPTS: usize = 32;

    let mut event_ids = (0..motion_events.len())
        .map(|idx| to_u32(idx, "global_event_index"))
        .collect::<Result<Vec<_>, _>>()?;

    let requires_non_adjacent_simultaneous = motion_events
        .iter()
        .map(|event| event.time_slot)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        < motion_events.len();

    for attempt in 0..MAX_RESHUFFLE_ATTEMPTS {
        shuffle_u32(rng, &mut event_ids);
        for (event, event_id) in motion_events.iter_mut().zip(event_ids.iter().copied()) {
            event.global_event_index = event_id;
        }
        if !requires_non_adjacent_simultaneous
            || has_non_adjacent_simultaneous_pair(motion_events)
            || attempt + 1 == MAX_RESHUFFLE_ATTEMPTS
        {
            break;
        }
    }
    Ok(())
}

fn has_non_adjacent_simultaneous_pair(motion_events: &[MotionEvent]) -> bool {
    let mut slot_to_ids: std::collections::BTreeMap<u32, Vec<u32>> =
        std::collections::BTreeMap::new();
    for event in motion_events {
        slot_to_ids
            .entry(event.time_slot)
            .or_default()
            .push(event.global_event_index);
    }

    slot_to_ids.values().any(|ids| {
        if ids.len() < 2 {
            return false;
        }
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let a = ids[i];
                let b = ids[j];
                let diff = a.abs_diff(b);
                if diff != 1 {
                    return true;
                }
            }
        }
        false
    })
}

fn shuffle_u32(rng: &mut ChaCha8Rng, values: &mut [u32]) {
    for i in (1..values.len()).rev() {
        let upper = u64::try_from(i + 1).expect("shuffle upper bound should fit u64");
        let j = usize::try_from(rng.next_u64() % upper).expect("shuffle index should fit usize");
        values.swap(i, j);
    }
}

fn build_accounting(
    motion_events: &[MotionEvent],
    scene: &SceneConfig,
    motion_events_per_shape: &[u16],
) -> Result<MotionEventAccounting, SceneGenerationError> {
    let generated_total = to_u32(motion_events.len(), "generated_total")?;
    if let Some(cap) = scene.n_motion_events_total {
        if generated_total > cap {
            return Err(SceneGenerationError::RandomizedEventAllocationExhausted {
                remaining: generated_total - cap,
                capacity: cap,
            });
        }
    }

    let expected_per_shape = motion_events_per_shape.to_vec();
    let mut generated_per_shape = vec![0u16; expected_per_shape.len()];
    for event in motion_events {
        if event.shape_index >= generated_per_shape.len() {
            return Err(SceneGenerationError::EventShapeIndexOutOfBounds {
                shape_index: event.shape_index,
            });
        }
        generated_per_shape[event.shape_index] = generated_per_shape[event.shape_index]
            .checked_add(1)
            .ok_or(SceneGenerationError::GeneratedPerShapeCountOverflow {
                shape_index: event.shape_index,
            })?;
    }

    for (shape_index, (generated, expected)) in generated_per_shape
        .iter()
        .zip(expected_per_shape.iter())
        .enumerate()
    {
        if generated != expected {
            return Err(SceneGenerationError::PerShapeEventCountMismatch {
                shape_index,
                generated: *generated,
                expected: *expected,
            });
        }
    }

    Ok(MotionEventAccounting {
        expected_total: generated_total,
        expected_slots: scene.n_motion_slots,
        generated_total,
        expected_per_shape,
        generated_per_shape,
    })
}

fn resolve_motion_events_per_shape(
    scene: &SceneConfig,
    rng: &mut ChaCha8Rng,
) -> Result<Vec<u16>, SceneGenerationError> {
    if !scene.randomize_motion_events_per_shape {
        return Ok(scene.motion_events_per_shape.clone());
    }
    if let Some(ranges) = scene.motion_events_per_shape_random_ranges.as_ref() {
        return randomized_motion_events_per_shape_with_ranges(scene, ranges, rng);
    }
    randomized_motion_events_per_shape(scene, rng)
}

fn randomized_motion_events_per_shape(
    scene: &SceneConfig,
    rng: &mut ChaCha8Rng,
) -> Result<Vec<u16>, SceneGenerationError> {
    let n_shapes = usize::from(scene.n_shapes);
    let slot_limit = scene.n_motion_slots.min(u32::from(u16::MAX));
    let mut counts = vec![0u16; n_shapes];
    for count in counts.iter_mut() {
        let sampled = rng.next_u64() % u64::from(slot_limit.saturating_add(1));
        *count = u16::try_from(sampled).expect("sampled count bounded by u16::MAX");
    }
    reduce_counts_to_budget(scene, &mut counts, None, rng)?;
    Ok(counts)
}

fn randomized_motion_events_per_shape_with_ranges(
    scene: &SceneConfig,
    ranges: &[crate::config::MotionEventsPerShapeRange],
    rng: &mut ChaCha8Rng,
) -> Result<Vec<u16>, SceneGenerationError> {
    if ranges.is_empty() {
        return Ok(Vec::new());
    }

    let mut counts = Vec::with_capacity(ranges.len());
    for range in ranges {
        let span = u32::from(range.max) - u32::from(range.min);
        let offset = if span == 0 {
            0
        } else {
            u32::try_from(rng.next_u64() % u64::from(span + 1)).expect("span fits u32")
        };
        let value = u32::from(range.min) + offset;
        counts.push(u16::try_from(value).expect("range value must fit u16"));
    }
    let minimums = ranges.iter().map(|range| range.min).collect::<Vec<_>>();
    reduce_counts_to_budget(scene, &mut counts, Some(&minimums), rng)?;
    Ok(counts)
}

fn reduce_counts_to_budget(
    scene: &SceneConfig,
    counts: &mut [u16],
    minimums: Option<&[u16]>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SceneGenerationError> {
    let n_shapes_u32 =
        u32::try_from(counts.len()).map_err(|_| SceneGenerationError::IndexOverflow {
            field: "n_shapes",
            value: counts.len(),
        })?;
    let mode_capacity = if scene.allow_simultaneous {
        scene.n_motion_slots.saturating_mul(n_shapes_u32)
    } else {
        scene.n_motion_slots
    };
    let budget = scene
        .n_motion_events_total
        .unwrap_or(mode_capacity)
        .min(mode_capacity);
    let mut total = counts.iter().copied().map(u32::from).sum::<u32>();
    if total <= budget {
        return Ok(());
    }

    let minimums = minimums
        .map(|mins| mins.to_vec())
        .unwrap_or_else(|| vec![0u16; counts.len()]);
    while total > budget {
        let mut eligible = Vec::new();
        for (shape_index, count) in counts.iter().copied().enumerate() {
            if count > minimums[shape_index] {
                eligible.push(shape_index);
            }
        }
        if eligible.is_empty() {
            return Err(SceneGenerationError::RandomizedEventAllocationExhausted {
                remaining: total - budget,
                capacity: budget,
            });
        }
        let pick = usize::try_from(
            rng.next_u64() % u64::try_from(eligible.len()).expect("eligible len fits u64"),
        )
        .expect("eligible pick fits usize");
        let selected = eligible[pick];
        counts[selected] -= 1;
        total -= 1;
    }
    Ok(())
}

fn to_u32(value: usize, field: &'static str) -> Result<u32, SceneGenerationError> {
    value
        .try_into()
        .map_err(|_| SceneGenerationError::IndexOverflow { field, value })
}

fn to_u16(value: usize, field: &'static str) -> Result<u16, SceneGenerationError> {
    value
        .try_into()
        .map_err(|_| SceneGenerationError::IndexOverflow { field, value })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sample_config() -> ShapeFlowConfig {
        ShapeFlowConfig {
            schema_version: crate::config::CURRENT_SCHEMA_VERSION,
            master_seed: 1234,
            generation_profile: None,
            scene: crate::config::SceneConfig {
                resolution: 512,
                n_shapes: 2,
                trajectory_complexity: 2,
                event_duration_frames: 12,
                easing_family: EasingFamily::EaseInOut,
                n_motion_slots: 6,
                motion_events_per_shape: vec![3, 3],
                n_motion_events_total: None,
                allow_simultaneous: true,
                shape_identity_assignment: crate::config::ShapeIdentityAssignment::IndexLocked,
                randomize_motion_events_per_shape: false,
                motion_events_per_shape_random_ranges: None,
                sound_sample_rate_hz: 44_100,
                sound_frames_per_second: 24,
                sound_modulation_depth_per_mille: 250,
                sound_channel_mapping: crate::config::SoundChannelMapping::StereoAlternating,
                text_reference_frame: crate::config::TextReferenceFrame::Canonical,
                text_synonym_rate: 0.0,
                text_typo_rate: 0.0,
                video_keyframe_border: false,
                image_frame_scatter: false,
                image_arrow_type: crate::config::ImageArrowType::Next,
            },
            positional_landscape: crate::config::PositionalLandscapeConfig {
                x_nonlinearity: crate::config::AxisNonlinearityFamily::Sigmoid,
                y_nonlinearity: crate::config::AxisNonlinearityFamily::Tanh,
                x_steepness: 3.0,
                y_steepness: 2.0,
            },
            site_graph: crate::config::SiteGraphConfig {
                site_k: 10,
                lambda2_min: 0.05,
                validation_scene_count: 32,
                lambda2_iterations: 64,
            },
            parallelism: crate::config::ParallelismConfig { num_threads: 4 },
        }
    }

    fn assert_global_event_indices_cover_contiguous_range(motion_events: &[MotionEvent]) {
        let mut observed = motion_events
            .iter()
            .map(|event| event.global_event_index)
            .collect::<Vec<_>>();
        observed.sort_unstable();
        let expected = (0..motion_events.len())
            .map(|idx| idx as u32)
            .collect::<Vec<_>>();
        assert_eq!(observed, expected);
    }

    fn assert_shape_event_indices_are_contiguous_per_shape(
        motion_events: &[MotionEvent],
        expected_per_shape: &[u16],
    ) {
        let mut per_shape_indices: BTreeMap<usize, Vec<u16>> = BTreeMap::new();
        for event in motion_events {
            per_shape_indices
                .entry(event.shape_index)
                .or_default()
                .push(event.shape_event_index);
        }

        for (shape_index, expected_count) in expected_per_shape.iter().copied().enumerate() {
            let observed = per_shape_indices.remove(&shape_index).unwrap_or_default();
            let expected = (0..expected_count).collect::<Vec<_>>();
            assert_eq!(observed, expected, "shape {shape_index} event-index drift");
        }
        assert!(
            per_shape_indices.is_empty(),
            "unexpected shape indices in generated events: {:?}",
            per_shape_indices.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn generate_scene_is_deterministic_for_same_parameters() {
        let cfg = sample_config();
        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 9,
            samples_per_event: 7,
            projection: SceneProjectionMode::SoftQuadrants,
        };

        let first = generate_scene(&params).expect("scene generation should succeed");
        let second = generate_scene(&params).expect("scene generation should succeed");
        assert_eq!(first, second);
    }

    #[test]
    fn generated_event_count_matches_config_totals() {
        let cfg = sample_config();
        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 11,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        assert_eq!(output.motion_events.len(), usize::from(6_u8));
        assert_eq!(output.accounting.expected_total, 6);
        assert_eq!(output.accounting.expected_slots, 6);
        assert_eq!(output.accounting.generated_total, 6);
        assert_eq!(output.accounting.expected_per_shape, vec![3, 3]);
        assert_eq!(output.accounting.generated_per_shape, vec![3, 3]);
    }

    #[test]
    fn generated_event_accounting_supports_non_uniform_three_shape_counts() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 4;
        cfg.scene.motion_events_per_shape = vec![1, 2, 4];
        cfg.scene.n_motion_events_total = None;

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 13,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        assert_eq!(output.motion_events.len(), 7);
        assert_eq!(output.accounting.expected_total, 7);
        assert_eq!(output.accounting.expected_slots, 4);
        assert_eq!(output.accounting.generated_total, 7);
        assert_eq!(output.accounting.expected_per_shape, vec![1, 2, 4]);
        assert_eq!(output.accounting.generated_per_shape, vec![1, 2, 4]);
    }

    #[test]
    fn randomized_motion_events_per_shape_is_seed_deterministic() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 12;
        cfg.scene.motion_events_per_shape = vec![4, 4, 4];
        cfg.scene.n_motion_events_total = Some(12);
        cfg.scene.allow_simultaneous = false;
        cfg.scene.randomize_motion_events_per_shape = true;

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 17,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let first = generate_scene(&params).expect("scene generation should succeed");
        let second = generate_scene(&params).expect("scene generation should succeed");
        assert_eq!(
            first.accounting.expected_per_shape,
            second.accounting.expected_per_shape
        );
        assert_eq!(
            first.accounting.generated_per_shape,
            second.accounting.generated_per_shape
        );
        assert_eq!(
            first
                .accounting
                .expected_per_shape
                .iter()
                .copied()
                .map(u32::from)
                .sum::<u32>(),
            first.accounting.generated_total
        );
        assert!(first.accounting.generated_total <= 12);
    }

    #[test]
    fn randomized_motion_events_per_shape_changes_with_scene_index() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 12;
        cfg.scene.motion_events_per_shape = vec![4, 4, 4];
        cfg.scene.n_motion_events_total = Some(12);
        cfg.scene.allow_simultaneous = false;
        cfg.scene.randomize_motion_events_per_shape = true;

        let first = generate_scene(&SceneGenerationParams {
            config: &cfg,
            scene_index: 19,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        })
        .expect("scene generation should succeed");
        let second = generate_scene(&SceneGenerationParams {
            config: &cfg,
            scene_index: 20,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        })
        .expect("scene generation should succeed");

        assert_ne!(
            first.accounting.expected_per_shape, second.accounting.expected_per_shape,
            "randomized per-shape counts should vary across scene index under deterministic RNG"
        );
    }

    #[test]
    fn randomized_motion_events_per_shape_with_ranges_is_seed_deterministic() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 12;
        cfg.scene.motion_events_per_shape = Vec::new();
        cfg.scene.n_motion_events_total = Some(12);
        cfg.scene.allow_simultaneous = false;
        cfg.scene.randomize_motion_events_per_shape = true;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            crate::config::MotionEventsPerShapeRange { min: 0, max: 12 },
            crate::config::MotionEventsPerShapeRange { min: 0, max: 12 },
            crate::config::MotionEventsPerShapeRange { min: 0, max: 12 },
        ]);

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 17,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let first = generate_scene(&params).expect("scene generation should succeed");
        let second = generate_scene(&params).expect("scene generation should succeed");
        assert_eq!(
            first.accounting.expected_per_shape,
            second.accounting.expected_per_shape
        );
        assert_eq!(
            first.accounting.generated_per_shape,
            second.accounting.generated_per_shape
        );
        assert_eq!(
            first
                .accounting
                .expected_per_shape
                .iter()
                .copied()
                .map(u32::from)
                .sum::<u32>(),
            first.accounting.generated_total
        );
        assert!(first.accounting.generated_total <= 12);
    }

    #[test]
    fn randomized_motion_events_per_shape_with_ranges_changes_with_scene_index() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 12;
        cfg.scene.motion_events_per_shape = Vec::new();
        cfg.scene.n_motion_events_total = Some(12);
        cfg.scene.allow_simultaneous = false;
        cfg.scene.randomize_motion_events_per_shape = true;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            crate::config::MotionEventsPerShapeRange { min: 0, max: 12 },
            crate::config::MotionEventsPerShapeRange { min: 0, max: 12 },
            crate::config::MotionEventsPerShapeRange { min: 0, max: 12 },
        ]);

        let first = generate_scene(&SceneGenerationParams {
            config: &cfg,
            scene_index: 19,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        })
        .expect("scene generation should succeed");
        let second = generate_scene(&SceneGenerationParams {
            config: &cfg,
            scene_index: 20,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        })
        .expect("scene generation should succeed");

        assert_ne!(
            first.accounting.expected_per_shape, second.accounting.expected_per_shape,
            "randomized per-shape counts should vary across scene index under deterministic RNG"
        );
    }

    #[test]
    fn randomized_motion_events_per_shape_with_ranges_respects_bounds_and_total() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 9;
        cfg.scene.motion_events_per_shape = Vec::new();
        cfg.scene.n_motion_events_total = Some(9);
        cfg.scene.allow_simultaneous = false;
        cfg.scene.randomize_motion_events_per_shape = true;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            crate::config::MotionEventsPerShapeRange { min: 0, max: 6 },
            crate::config::MotionEventsPerShapeRange { min: 2, max: 4 },
            crate::config::MotionEventsPerShapeRange { min: 1, max: 3 },
        ]);

        let output = generate_scene(&SceneGenerationParams {
            config: &cfg,
            scene_index: 23,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        })
        .expect("scene generation should succeed");

        assert!(output.accounting.generated_total <= 9);
        let expected = &output.accounting.expected_per_shape;
        assert_eq!(expected.len(), 3);
        assert!((0..=6).contains(&u32::from(expected[0])));
        assert!((2..=4).contains(&u32::from(expected[1])));
        assert!((1..=3).contains(&u32::from(expected[2])));
        assert_eq!(
            expected.iter().copied().map(u32::from).sum::<u32>(),
            output.accounting.generated_total
        );
    }

    #[test]
    fn generated_events_include_scene_metadata() {
        let cfg = sample_config();
        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 12,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        for event in output.motion_events {
            assert_eq!(event.duration_frames, cfg.scene.event_duration_frames);
            assert_eq!(event.easing, cfg.scene.easing_family);
        }
    }

    #[test]
    fn soft_quadrants_projection_matches_per_shape_point_count() {
        let cfg = sample_config();
        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 5,
            samples_per_event: 4,
            projection: SceneProjectionMode::SoftQuadrants,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        for shape in output.shape_paths {
            let memberships = shape.soft_memberships.expect("soft quadrants expected");
            assert_eq!(shape.trajectory_points.len(), memberships.len());
        }
    }

    #[test]
    fn simultaneous_mode_groups_events_by_time_slot() {
        let cfg = sample_config();
        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 3,
            samples_per_event: 3,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        let actual_slots: Vec<u32> = output
            .motion_events
            .iter()
            .map(|event| event.time_slot)
            .collect();
        assert!(
            actual_slots
                .iter()
                .all(|slot| *slot < cfg.scene.n_motion_slots)
        );
        assert_global_event_indices_cover_contiguous_range(&output.motion_events);
        assert_shape_event_indices_are_contiguous_per_shape(&output.motion_events, &[3, 3]);
        let distinct_slot_count = actual_slots
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert!(
            distinct_slot_count
                <= usize::try_from(cfg.scene.n_motion_slots).expect("slots must fit usize")
        );
        assert!(
            has_non_adjacent_simultaneous_pair(&output.motion_events),
            "simultaneous event ids should not always collapse to adjacent pairs"
        );
    }

    #[test]
    fn sequential_mode_uses_monotonic_time_slots() {
        let mut cfg = sample_config();
        cfg.scene.allow_simultaneous = false;

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 3,
            samples_per_event: 3,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        let mut previous = None;
        for event in &output.motion_events {
            if let Some(prev) = previous {
                assert!(event.time_slot >= prev);
            }
            assert!(event.time_slot < cfg.scene.n_motion_slots);
            previous = Some(event.time_slot);
        }
        assert_global_event_indices_cover_contiguous_range(&output.motion_events);
        assert_shape_event_indices_are_contiguous_per_shape(&output.motion_events, &[3, 3]);
    }

    #[test]
    fn simultaneous_mode_preserves_per_shape_event_index_sequences_for_non_uniform_counts() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 4;
        cfg.scene.motion_events_per_shape = vec![1, 2, 4];
        cfg.scene.n_motion_events_total = None;
        cfg.scene.allow_simultaneous = true;

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 17,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        assert_global_event_indices_cover_contiguous_range(&output.motion_events);
        assert_shape_event_indices_are_contiguous_per_shape(&output.motion_events, &[1, 2, 4]);
    }

    #[test]
    fn sequential_mode_preserves_per_shape_event_index_sequences_for_non_uniform_counts() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 7;
        cfg.scene.motion_events_per_shape = vec![1, 2, 4];
        cfg.scene.n_motion_events_total = None;
        cfg.scene.allow_simultaneous = false;

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 19,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        let mut previous = None;
        for event in &output.motion_events {
            if let Some(prev) = previous {
                assert!(event.time_slot >= prev);
            }
            assert!(event.time_slot < cfg.scene.n_motion_slots);
            previous = Some(event.time_slot);
        }
        assert_global_event_indices_cover_contiguous_range(&output.motion_events);
        assert_shape_event_indices_are_contiguous_per_shape(&output.motion_events, &[1, 2, 4]);
    }

    #[test]
    fn rejects_zero_samples_per_event() {
        let cfg = sample_config();
        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 1,
            samples_per_event: 0,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let err = generate_scene(&params).expect_err("samples_per_event=0 should fail");
        assert!(matches!(
            err,
            SceneGenerationError::InvalidSamplesPerEvent {
                samples_per_event: 0
            }
        ));
    }

    #[test]
    fn invalid_config_propagates_config_error() {
        let mut cfg = sample_config();
        cfg.positional_landscape.x_steepness = -1.0;
        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 2,
            samples_per_event: 4,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let err = generate_scene(&params).expect_err("invalid config should fail");
        assert!(matches!(
            err,
            SceneGenerationError::Config(ConfigError::InvalidSteepness {
                axis: "x",
                value: -1.0
            })
        ));
    }

    #[test]
    fn varying_trajectory_complexity_changes_path_geometry_without_altering_event_accounting() {
        let mut cfg_low = sample_config();
        cfg_low.scene.trajectory_complexity = 1;
        let mut cfg_high = sample_config();
        cfg_high.scene.trajectory_complexity = 4;

        let low = generate_scene(&SceneGenerationParams {
            config: &cfg_low,
            scene_index: 29,
            samples_per_event: 8,
            projection: SceneProjectionMode::TrajectoryOnly,
        })
        .expect("scene generation should succeed");
        let high = generate_scene(&SceneGenerationParams {
            config: &cfg_high,
            scene_index: 29,
            samples_per_event: 8,
            projection: SceneProjectionMode::TrajectoryOnly,
        })
        .expect("scene generation should succeed");

        assert_eq!(low.accounting, high.accounting);
        assert_eq!(low.motion_events, high.motion_events);
        assert_eq!(low.shape_paths.len(), high.shape_paths.len());

        let samples_per_event = 8usize;
        for (low_path, high_path) in low.shape_paths.iter().zip(high.shape_paths.iter()) {
            assert_eq!(low_path.shape_index, high_path.shape_index);
            assert_eq!(
                low_path.trajectory_points.len(),
                high_path.trajectory_points.len()
            );
            let boundary_count =
                usize::from(low.accounting.expected_per_shape[low_path.shape_index]);
            for boundary in 0..=boundary_count {
                let idx = boundary * samples_per_event;
                let low_point = low_path.trajectory_points[idx];
                let high_point = high_path.trajectory_points[idx];
                assert!((low_point.x - high_point.x).abs() <= 1e-9);
                assert!((low_point.y - high_point.y).abs() <= 1e-9);
            }
        }

        let changed_internal_point = low
            .shape_paths
            .iter()
            .zip(high.shape_paths.iter())
            .flat_map(|(low_path, high_path)| {
                low_path
                    .trajectory_points
                    .iter()
                    .zip(high_path.trajectory_points.iter())
                    .enumerate()
                    .filter(|(idx, _)| idx % samples_per_event != 0)
                    .map(|(_, (low_point, high_point))| {
                        (low_point.x - high_point.x).abs() > 1e-9
                            || (low_point.y - high_point.y).abs() > 1e-9
                    })
            })
            .any(|changed| changed);
        assert!(
            changed_internal_point,
            "trajectory complexity should change at least one non-boundary sampled point"
        );
    }
}
