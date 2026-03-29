use crate::config::{ConfigError, EasingFamily, SceneConfig, ShapeFlowConfig};
use crate::landscape::{LandscapeError, SoftQuadrantMembership, positional_identity};
use crate::seed_schedule::SceneSeedSchedule;
use crate::trajectory::{NormalizedPoint, TrajectoryError, sample_random_linear_path_points};

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
    pub generated_total: u32,
    pub expected_per_shape: Vec<u16>,
    pub generated_per_shape: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneGenerationOutput {
    pub scene_index: u64,
    pub schedule: SceneSeedSchedule,
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
        "generated motion event count ({generated}) must equal scene.n_motion_events_total ({expected})"
    )]
    GeneratedEventCountMismatch { generated: u32, expected: u32 },
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

    let mut shape_paths = Vec::with_capacity(params.config.scene.motion_events_per_shape.len());
    for (shape_index, event_count) in params
        .config
        .scene
        .motion_events_per_shape
        .iter()
        .copied()
        .enumerate()
    {
        let trajectory_points = sample_random_linear_path_points(
            &mut trajectory_rng,
            usize::from(event_count),
            params.samples_per_event,
        )?;
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

    let motion_events =
        build_motion_events(&shape_paths, &params.config.scene, params.samples_per_event)?;
    let accounting = build_accounting(&motion_events, &params.config.scene)?;

    Ok(SceneGenerationOutput {
        scene_index: params.scene_index,
        schedule,
        shape_paths,
        motion_events,
        accounting,
    })
}

fn build_motion_events(
    shape_paths: &[SceneShapePath],
    scene: &SceneConfig,
    samples_per_event: usize,
) -> Result<Vec<MotionEvent>, SceneGenerationError> {
    let mut motion_events =
        Vec::with_capacity(usize::try_from(scene.n_motion_events_total).map_err(|_| {
            SceneGenerationError::IndexOverflow {
                field: "scene.n_motion_events_total",
                value: usize::MAX,
            }
        })?);

    if scene.allow_simultaneous {
        let max_events_per_shape = scene
            .motion_events_per_shape
            .iter()
            .copied()
            .max()
            .unwrap_or(0);

        for local_event_index in 0..usize::from(max_events_per_shape) {
            for shape_index in 0..shape_paths.len() {
                let events_for_shape = usize::from(scene.motion_events_per_shape[shape_index]);
                if local_event_index >= events_for_shape {
                    continue;
                }

                let time_slot = to_u32(local_event_index, "time_slot")?;
                let global_event_index = to_u32(motion_events.len(), "global_event_index")?;
                motion_events.push(motion_event_from_shape_path(
                    &shape_paths[shape_index],
                    local_event_index,
                    samples_per_event,
                    global_event_index,
                    time_slot,
                    scene.event_duration_frames,
                    scene.easing_family,
                )?);
            }
        }
    } else {
        for shape_index in 0..shape_paths.len() {
            let events_for_shape = usize::from(scene.motion_events_per_shape[shape_index]);
            for local_event_index in 0..events_for_shape {
                let global_event_index = to_u32(motion_events.len(), "global_event_index")?;
                motion_events.push(motion_event_from_shape_path(
                    &shape_paths[shape_index],
                    local_event_index,
                    samples_per_event,
                    global_event_index,
                    global_event_index,
                    scene.event_duration_frames,
                    scene.easing_family,
                )?);
            }
        }
    }

    Ok(motion_events)
}

fn motion_event_from_shape_path(
    shape_path: &SceneShapePath,
    shape_event_index: usize,
    samples_per_event: usize,
    global_event_index: u32,
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
        global_event_index,
        time_slot,
        shape_index: shape_path.shape_index,
        shape_event_index: to_u16(shape_event_index, "shape_event_index")?,
        start_point,
        end_point,
        duration_frames,
        easing,
    })
}

fn build_accounting(
    motion_events: &[MotionEvent],
    scene: &SceneConfig,
) -> Result<MotionEventAccounting, SceneGenerationError> {
    let generated_total = to_u32(motion_events.len(), "generated_total")?;
    if generated_total != scene.n_motion_events_total {
        return Err(SceneGenerationError::GeneratedEventCountMismatch {
            generated: generated_total,
            expected: scene.n_motion_events_total,
        });
    }

    let expected_per_shape = scene.motion_events_per_shape.clone();
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
        expected_total: scene.n_motion_events_total,
        generated_total,
        expected_per_shape,
        generated_per_shape,
    })
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
            schema_version: 1,
            master_seed: 1234,
            scene: crate::config::SceneConfig {
                resolution: 512,
                n_shapes: 2,
                trajectory_complexity: 2,
                event_duration_frames: 12,
                easing_family: EasingFamily::EaseInOut,
                motion_events_per_shape: vec![3, 3],
                n_motion_events_total: 6,
                allow_simultaneous: true,
                sound_sample_rate_hz: 44_100,
                sound_frames_per_second: 24,
                sound_modulation_depth_per_mille: 250,
                sound_channel_mapping: crate::config::SoundChannelMapping::StereoAlternating,
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
            split: crate::config::SplitConfig {
                policy: crate::config::SplitPolicyConfig::Standard,
            },
            parallelism: crate::config::ParallelismConfig { num_threads: 4 },
        }
    }

    fn assert_global_event_indices_are_contiguous(motion_events: &[MotionEvent]) {
        for (expected_index, event) in motion_events.iter().enumerate() {
            assert_eq!(event.global_event_index, expected_index as u32);
        }
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
        assert_eq!(output.accounting.generated_total, 6);
        assert_eq!(output.accounting.expected_per_shape, vec![3, 3]);
        assert_eq!(output.accounting.generated_per_shape, vec![3, 3]);
    }

    #[test]
    fn generated_event_accounting_supports_non_uniform_three_shape_counts() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.motion_events_per_shape = vec![1, 2, 4];
        cfg.scene.n_motion_events_total = 7;

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 13,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        assert_eq!(output.motion_events.len(), 7);
        assert_eq!(output.accounting.expected_total, 7);
        assert_eq!(output.accounting.generated_total, 7);
        assert_eq!(output.accounting.expected_per_shape, vec![1, 2, 4]);
        assert_eq!(output.accounting.generated_per_shape, vec![1, 2, 4]);
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
        let expected_slots = vec![0, 0, 1, 1, 2, 2];
        let actual_slots: Vec<u32> = output
            .motion_events
            .iter()
            .map(|event| event.time_slot)
            .collect();
        assert_eq!(actual_slots, expected_slots);
        assert_global_event_indices_are_contiguous(&output.motion_events);
        assert_shape_event_indices_are_contiguous_per_shape(&output.motion_events, &[3, 3]);
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
        for (idx, event) in output.motion_events.iter().enumerate() {
            assert_eq!(event.time_slot, idx as u32);
        }
        assert_global_event_indices_are_contiguous(&output.motion_events);
        assert_shape_event_indices_are_contiguous_per_shape(&output.motion_events, &[3, 3]);
    }

    #[test]
    fn simultaneous_mode_preserves_per_shape_event_index_sequences_for_non_uniform_counts() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.motion_events_per_shape = vec![1, 2, 4];
        cfg.scene.n_motion_events_total = 7;
        cfg.scene.allow_simultaneous = true;

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 17,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        assert_global_event_indices_are_contiguous(&output.motion_events);
        assert_shape_event_indices_are_contiguous_per_shape(&output.motion_events, &[1, 2, 4]);
    }

    #[test]
    fn sequential_mode_preserves_per_shape_event_index_sequences_for_non_uniform_counts() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.motion_events_per_shape = vec![1, 2, 4];
        cfg.scene.n_motion_events_total = 7;
        cfg.scene.allow_simultaneous = false;

        let params = SceneGenerationParams {
            config: &cfg,
            scene_index: 19,
            samples_per_event: 5,
            projection: SceneProjectionMode::TrajectoryOnly,
        };

        let output = generate_scene(&params).expect("scene generation should succeed");
        for (idx, event) in output.motion_events.iter().enumerate() {
            assert_eq!(event.time_slot, idx as u32);
        }
        assert_global_event_indices_are_contiguous(&output.motion_events);
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
}
