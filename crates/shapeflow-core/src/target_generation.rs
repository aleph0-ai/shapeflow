use std::collections::BTreeSet;

use crate::ShapeFlowConfig;
use crate::landscape::SoftQuadrantMembership;
use crate::scene_generation::{
    SceneGenerationError, SceneGenerationOutput, SceneGenerationParams, SceneProjectionMode,
    generate_scene,
};
use crate::tabular_encoding::canonical_class_rank_for_scene_seed;

const SIMPLEX_SUM_TOLERANCE: f64 = 1.0e-9;
const INTEGER_COMPONENT_TOLERANCE: f64 = 1.0e-12;
const LARGEST_EVENT_DISTANCE_TIE_TOLERANCE: f64 = 1.0e-12;

const TASK_LARGEST_MOTION_EVENT_SHAPE: &str = "lme0000";

#[derive(Debug, thiserror::Error)]
pub enum TargetGenerationError {
    #[error(
        "shape {shape_index} is missing soft memberships; generate scene with SoftQuadrants projection"
    )]
    MissingSoftMemberships { shape_index: usize },
    #[error("shape {shape_index} has no soft memberships")]
    EmptySoftMemberships { shape_index: usize },
    #[error("shape {shape_index} has no generated target segments")]
    EmptyTargetSegments { shape_index: usize },
    #[error("shape {shape_index} has no motion events")]
    ShapeHasNoMotionEvents { shape_index: usize },
    #[error(
        "target component out of range for shape {shape_index}, segment {segment_index}, component {component_index}: {value}"
    )]
    SegmentComponentOutOfRange {
        shape_index: usize,
        segment_index: usize,
        component_index: usize,
        value: f64,
    },
    #[error(
        "target simplex sum mismatch for shape {shape_index}, segment {segment_index}: sum={sum}, tolerance={tolerance}"
    )]
    SegmentSimplexMismatch {
        shape_index: usize,
        segment_index: usize,
        sum: f64,
        tolerance: f64,
    },
    #[error("target set collapsed into hard-only segments (all one-hot-like vectors)")]
    HardOnlyTargets,
    #[error("shape identity generation failed for shape_index={shape_index}: {message}")]
    ShapeIdentity { shape_index: usize, message: String },
    #[error("canonical class rank generation failed for shape_index={shape_index}: {message}")]
    CanonicalClassRank { shape_index: usize, message: String },
    #[error("generated target {task_id} has no segments")]
    EmptyGeneratedTargetSegments { task_id: String },
    #[error(
        "generated target {task_id} has mismatched segment width at segment {segment_index}: expected {expected}, found {found}"
    )]
    GeneratedTargetSegmentWidthMismatch {
        task_id: String,
        segment_index: usize,
        expected: usize,
        found: usize,
    },
    #[error(
        "generated target {task_id} has non-finite value at segment {segment_index}, component {component_index}: {value}"
    )]
    NonFiniteGeneratedTargetValue {
        task_id: String,
        segment_index: usize,
        component_index: usize,
        value: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderedQuadrantPassageTarget {
    pub shape_index: usize,
    pub segments: Vec<SoftQuadrantMembership>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetValidationReport {
    pub shape_target_count: usize,
    pub total_segments: usize,
    pub hard_segment_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedTarget {
    pub task_id: String,
    pub segments: Vec<Vec<f64>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratedTargetValidationReport {
    pub target_count: usize,
    pub total_segments: usize,
    pub total_values: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SceneTargetGenerationError {
    #[error("samples_per_event must be > 0")]
    InvalidSamplesPerEvent,
    #[error("scene generation failed: {0}")]
    SceneGeneration(#[from] SceneGenerationError),
    #[error("target generation failed: {0}")]
    TargetGeneration(#[from] TargetGenerationError),
}

pub fn generate_scene_targets_for_index(
    config: &ShapeFlowConfig,
    scene_index: u64,
    samples_per_event: usize,
) -> Result<Vec<GeneratedTarget>, SceneTargetGenerationError> {
    if samples_per_event == 0 {
        return Err(SceneTargetGenerationError::InvalidSamplesPerEvent);
    }

    let params = SceneGenerationParams {
        config,
        scene_index,
        samples_per_event,
        projection: SceneProjectionMode::SoftQuadrants,
    };
    let scene = generate_scene(&params)?;
    generate_all_scene_targets(&scene).map_err(SceneTargetGenerationError::TargetGeneration)
}

pub fn expected_target_task_ids(shape_count: usize) -> Vec<String> {
    let mut task_ids = Vec::with_capacity(shape_count.saturating_mul(3).saturating_add(1));
    for shape_index in 0..shape_count {
        task_ids.push(format!("oqp{shape_index:04}"));
        task_ids.push(format!("xct{shape_index:04}"));
        task_ids.push(format!("zqh{shape_index:04}"));
    }
    task_ids.push(TASK_LARGEST_MOTION_EVENT_SHAPE.to_string());
    task_ids
}

pub fn generate_ordered_quadrant_passage_targets(
    scene: &SceneGenerationOutput,
) -> Result<Vec<OrderedQuadrantPassageTarget>, TargetGenerationError> {
    let mut targets = Vec::with_capacity(scene.shape_paths.len());
    for shape_path in &scene.shape_paths {
        let memberships = shape_path.soft_memberships.as_ref().ok_or(
            TargetGenerationError::MissingSoftMemberships {
                shape_index: shape_path.shape_index,
            },
        )?;
        if memberships.is_empty() {
            return Err(TargetGenerationError::EmptySoftMemberships {
                shape_index: shape_path.shape_index,
            });
        }

        let segments = compress_memberships_into_segments(memberships);
        targets.push(OrderedQuadrantPassageTarget {
            shape_index: shape_path.shape_index,
            segments,
        });
    }
    Ok(targets)
}

pub fn validate_ordered_quadrant_passage_targets(
    targets: &[OrderedQuadrantPassageTarget],
) -> Result<TargetValidationReport, TargetGenerationError> {
    let mut total_segments = 0usize;
    let mut hard_segment_count = 0usize;

    for target in targets {
        if target.segments.is_empty() {
            return Err(TargetGenerationError::EmptyTargetSegments {
                shape_index: target.shape_index,
            });
        }

        for (segment_index, segment) in target.segments.iter().enumerate() {
            let values = segment.as_array();
            for (component_index, value) in values.iter().copied().enumerate() {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    return Err(TargetGenerationError::SegmentComponentOutOfRange {
                        shape_index: target.shape_index,
                        segment_index,
                        component_index,
                        value,
                    });
                }
            }

            let sum: f64 = values.iter().sum();
            if (sum - 1.0).abs() > SIMPLEX_SUM_TOLERANCE {
                return Err(TargetGenerationError::SegmentSimplexMismatch {
                    shape_index: target.shape_index,
                    segment_index,
                    sum,
                    tolerance: SIMPLEX_SUM_TOLERANCE,
                });
            }

            if is_hard_segment(values) {
                hard_segment_count += 1;
            }
            total_segments += 1;
        }
    }

    if total_segments > 0 && hard_segment_count == total_segments {
        return Err(TargetGenerationError::HardOnlyTargets);
    }

    Ok(TargetValidationReport {
        shape_target_count: targets.len(),
        total_segments,
        hard_segment_count,
    })
}

pub fn generate_all_scene_targets(
    scene: &SceneGenerationOutput,
) -> Result<Vec<GeneratedTarget>, TargetGenerationError> {
    let mut all_targets = Vec::new();

    let mut oqp_targets = generate_ordered_quadrant_passage_targets(scene)?;
    oqp_targets.sort_by_key(|target| target.shape_index);
    for target in oqp_targets {
        let memberships = scene.shape_paths[target.shape_index]
            .soft_memberships
            .as_ref()
            .ok_or(TargetGenerationError::MissingSoftMemberships {
                shape_index: target.shape_index,
            })?;
        let dominant = dominant_quadrants_for_memberships(memberships);
        let transition_count = count_quadrant_transitions(&dominant);
        let (query_move_count, query_quadrant) =
            deterministic_quadrant_after_moves(scene, target.shape_index)?;

        all_targets.push(GeneratedTarget {
            task_id: format!("oqp{:04}", target.shape_index),
            segments: target
                .segments
                .iter()
                .map(|segment| segment.as_array().to_vec())
                .collect(),
        });
        all_targets.push(GeneratedTarget {
            task_id: format!("xct{:04}", target.shape_index),
            segments: vec![vec![transition_count as f64]],
        });
        all_targets.push(GeneratedTarget {
            task_id: format!("zqh{:04}", target.shape_index),
            segments: vec![vec![query_move_count as f64, query_quadrant as f64]],
        });
    }

    let mut canonical_ranks = Vec::with_capacity(scene.shape_paths.len());
    for shape_index in 0..scene.shape_paths.len() {
        let rank = canonical_class_rank_for_scene_seed(
            scene.schedule.scene_layout,
            scene.shape_identity_assignment,
            shape_index,
        )
        .map_err(|error| TargetGenerationError::CanonicalClassRank {
            shape_index,
            message: error.to_string(),
        })?;
        canonical_ranks.push(rank);
    }

    let fallback_rank = canonical_ranks.iter().copied().min().unwrap_or(0u8);
    let mut best_distance_squared = -1.0_f64;
    let mut winner_rank = fallback_rank;
    for event in &scene.motion_events {
        let dx = event.end_point.x - event.start_point.x;
        let dy = event.end_point.y - event.start_point.y;
        let distance_squared = dx * dx + dy * dy;
        let rank = canonical_ranks
            .get(event.shape_index)
            .copied()
            .unwrap_or(fallback_rank);

        if distance_squared > best_distance_squared + LARGEST_EVENT_DISTANCE_TIE_TOLERANCE {
            best_distance_squared = distance_squared;
            winner_rank = rank;
            continue;
        }
        if (distance_squared - best_distance_squared).abs() <= LARGEST_EVENT_DISTANCE_TIE_TOLERANCE
            && rank < winner_rank
        {
            winner_rank = rank;
        }
    }
    all_targets.push(GeneratedTarget {
        task_id: TASK_LARGEST_MOTION_EVENT_SHAPE.to_string(),
        segments: vec![vec![f64::from(winner_rank)]],
    });

    all_targets.sort_by(|left, right| left.task_id.cmp(&right.task_id));
    Ok(all_targets)
}

pub fn validate_generated_targets(
    targets: &[GeneratedTarget],
) -> Result<GeneratedTargetValidationReport, TargetGenerationError> {
    let mut target_ids = BTreeSet::new();
    let mut total_segments = 0usize;
    let mut total_values = 0usize;

    for target in targets {
        target_ids.insert(target.task_id.clone());
        if target.segments.is_empty() {
            return Err(TargetGenerationError::EmptyGeneratedTargetSegments {
                task_id: target.task_id.clone(),
            });
        }
        let expected_width = target.segments[0].len();
        for (segment_index, segment) in target.segments.iter().enumerate() {
            if segment.len() != expected_width {
                return Err(TargetGenerationError::GeneratedTargetSegmentWidthMismatch {
                    task_id: target.task_id.clone(),
                    segment_index,
                    expected: expected_width,
                    found: segment.len(),
                });
            }
            for (component_index, value) in segment.iter().copied().enumerate() {
                if !value.is_finite() {
                    return Err(TargetGenerationError::NonFiniteGeneratedTargetValue {
                        task_id: target.task_id.clone(),
                        segment_index,
                        component_index,
                        value,
                    });
                }
            }
            total_values += segment.len();
        }
        total_segments += target.segments.len();
    }

    Ok(GeneratedTargetValidationReport {
        target_count: target_ids.len(),
        total_segments,
        total_values,
    })
}

fn compress_memberships_into_segments(
    memberships: &[SoftQuadrantMembership],
) -> Vec<SoftQuadrantMembership> {
    let mut result = Vec::new();
    let mut current_dominant = dominant_quadrant_index(&memberships[0]);
    let mut accum = [0.0_f64; 4];
    let mut count = 0usize;

    for membership in memberships {
        let dominant = dominant_quadrant_index(membership);
        if dominant != current_dominant && count > 0 {
            result.push(average_membership(accum, count));
            accum = [0.0; 4];
            count = 0;
            current_dominant = dominant;
        }

        let values = membership.as_array();
        for (index, value) in values.into_iter().enumerate() {
            accum[index] += value;
        }
        count += 1;
    }

    if count > 0 {
        result.push(average_membership(accum, count));
    }

    result
}

fn dominant_quadrants_for_memberships(memberships: &[SoftQuadrantMembership]) -> Vec<usize> {
    memberships
        .iter()
        .map(dominant_quadrant_index)
        .collect::<Vec<_>>()
}

fn count_quadrant_transitions(dominant_quadrants: &[usize]) -> usize {
    dominant_quadrants
        .windows(2)
        .filter(|window| window[0] != window[1])
        .count()
}

fn deterministic_quadrant_after_moves(
    scene: &SceneGenerationOutput,
    shape_index: usize,
) -> Result<(usize, usize), TargetGenerationError> {
    let mut shape_events = scene
        .motion_events
        .iter()
        .filter(|event| event.shape_index == shape_index)
        .collect::<Vec<_>>();
    if shape_events.is_empty() {
        return Err(TargetGenerationError::ShapeHasNoMotionEvents { shape_index });
    }
    shape_events.sort_by_key(|event| (event.shape_event_index, event.global_event_index));

    let queried_event_index = deterministic_move_query_index(
        scene.schedule.scene_layout,
        shape_index,
        shape_events.len(),
    );
    let queried_event = shape_events[queried_event_index];
    let queried_quadrant =
        hard_quadrant_from_point(queried_event.end_point.x, queried_event.end_point.y);

    Ok((queried_event_index + 1, queried_quadrant))
}

fn deterministic_move_query_index(
    scene_layout_seed: u64,
    shape_index: usize,
    event_count: usize,
) -> usize {
    debug_assert!(event_count > 0);
    let shape_mix = (shape_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mixed = splitmix64(scene_layout_seed ^ shape_mix ^ 0xD1B5_4A32_D192_ED03);
    (mixed % event_count as u64) as usize
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn hard_quadrant_from_point(x: f64, y: f64) -> usize {
    if y >= 0.0 {
        if x < 0.0 { 1 } else { 0 }
    } else if x <= 0.0 {
        2
    } else {
        3
    }
}

fn average_membership(sum_components: [f64; 4], count: usize) -> SoftQuadrantMembership {
    let inv_count = 1.0 / count as f64;
    SoftQuadrantMembership {
        q1: sum_components[0] * inv_count,
        q2: sum_components[1] * inv_count,
        q3: sum_components[2] * inv_count,
        q4: sum_components[3] * inv_count,
    }
}

fn dominant_quadrant_index(membership: &SoftQuadrantMembership) -> usize {
    let values = membership.as_array();
    let mut winner_idx = 0usize;
    let mut winner_value = values[0];

    for (idx, value) in values.iter().copied().enumerate().skip(1) {
        if value > winner_value {
            winner_idx = idx;
            winner_value = value;
        }
    }

    winner_idx
}

fn is_hard_segment(values: [f64; 4]) -> bool {
    let mut ones = 0usize;
    for value in values {
        if (value - 1.0).abs() <= INTEGER_COMPONENT_TOLERANCE {
            ones += 1;
            continue;
        }
        if value.abs() <= INTEGER_COMPONENT_TOLERANCE {
            continue;
        }
        return false;
    }
    ones == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_generation::{
        MotionEventAccounting, SceneGenerationParams, SceneProjectionMode, SceneShapePath,
        generate_scene,
    };
    use crate::{SceneSeedSchedule, ShapeFlowConfig};

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config must parse")
    }

    #[test]
    fn target_generation_requires_soft_projection() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 0,
            samples_per_event: 8,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let err = generate_ordered_quadrant_passage_targets(&scene)
            .expect_err("missing soft memberships should fail");
        assert!(matches!(
            err,
            TargetGenerationError::MissingSoftMemberships { shape_index: 0 }
        ));
    }

    #[test]
    fn ordered_targets_are_deterministic() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 3,
            samples_per_event: 12,
            projection: SceneProjectionMode::SoftQuadrants,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let first = generate_ordered_quadrant_passage_targets(&scene)
            .expect("target generation should succeed");
        let second = generate_ordered_quadrant_passage_targets(&scene)
            .expect("target generation should succeed");
        assert_eq!(first, second);
    }

    #[test]
    fn compression_uses_dominant_runs() {
        let scene = crate::scene_generation::SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_identity_assignment: crate::config::ShapeIdentityAssignment::IndexLocked,
            shape_paths: vec![SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![],
                soft_memberships: Some(vec![
                    SoftQuadrantMembership {
                        q1: 0.80,
                        q2: 0.10,
                        q3: 0.05,
                        q4: 0.05,
                    },
                    SoftQuadrantMembership {
                        q1: 0.70,
                        q2: 0.20,
                        q3: 0.05,
                        q4: 0.05,
                    },
                    SoftQuadrantMembership {
                        q1: 0.30,
                        q2: 0.55,
                        q3: 0.10,
                        q4: 0.05,
                    },
                    SoftQuadrantMembership {
                        q1: 0.20,
                        q2: 0.60,
                        q3: 0.10,
                        q4: 0.10,
                    },
                    SoftQuadrantMembership {
                        q1: 0.10,
                        q2: 0.10,
                        q3: 0.15,
                        q4: 0.65,
                    },
                ]),
            }],
            motion_events: vec![],
            accounting: MotionEventAccounting {
                expected_total: 0,
                expected_slots: 0,
                generated_total: 0,
                expected_per_shape: vec![],
                generated_per_shape: vec![],
            },
        };

        let targets =
            generate_ordered_quadrant_passage_targets(&scene).expect("target generation works");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].segments.len(), 3);

        let first = targets[0].segments[0];
        assert!((first.q1 - 0.75).abs() < 1.0e-12);
        assert!((first.q2 - 0.15).abs() < 1.0e-12);
        assert!((first.q3 - 0.05).abs() < 1.0e-12);
        assert!((first.q4 - 0.05).abs() < 1.0e-12);
    }

    #[test]
    fn validation_rejects_hard_only_segments() {
        let hard_only = vec![OrderedQuadrantPassageTarget {
            shape_index: 0,
            segments: vec![
                SoftQuadrantMembership {
                    q1: 1.0,
                    q2: 0.0,
                    q3: 0.0,
                    q4: 0.0,
                },
                SoftQuadrantMembership {
                    q1: 0.0,
                    q2: 1.0,
                    q3: 0.0,
                    q4: 0.0,
                },
            ],
        }];

        let err = validate_ordered_quadrant_passage_targets(&hard_only)
            .expect_err("hard-only targets should fail");
        assert!(matches!(err, TargetGenerationError::HardOnlyTargets));
    }

    #[test]
    fn bootstrap_scene_targets_validate() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 0,
            samples_per_event: 24,
            projection: SceneProjectionMode::SoftQuadrants,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let targets = generate_ordered_quadrant_passage_targets(&scene)
            .expect("target generation should succeed");
        let report = validate_ordered_quadrant_passage_targets(&targets)
            .expect("target validation should succeed");

        assert_eq!(
            report.shape_target_count,
            usize::from(config.scene.n_shapes)
        );
        assert!(report.total_segments >= report.shape_target_count);
        assert!(report.hard_segment_count < report.total_segments);
    }

    #[test]
    fn all_targets_include_per_shape_and_largest_motion_tasks() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 2,
            samples_per_event: 24,
            projection: SceneProjectionMode::SoftQuadrants,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let targets = generate_all_scene_targets(&scene).expect("target generation should succeed");
        let report =
            validate_generated_targets(&targets).expect("target validation should succeed");

        assert_eq!(
            report.target_count,
            expected_target_task_ids(scene.shape_paths.len()).len()
        );
        assert!(
            targets
                .iter()
                .any(|target| target.task_id == TASK_LARGEST_MOTION_EVENT_SHAPE)
        );
    }

    #[test]
    fn zqh_targets_are_valid_for_each_shape() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 4,
            samples_per_event: 24,
            projection: SceneProjectionMode::SoftQuadrants,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let targets = generate_all_scene_targets(&scene).expect("target generation should succeed");

        for shape_index in 0..scene.shape_paths.len() {
            let task_id = format!("zqh{shape_index:04}");
            let target = targets
                .iter()
                .find(|candidate| candidate.task_id == task_id)
                .expect("zqh target should exist for every shape");
            assert_eq!(
                target.segments.len(),
                1,
                "zqh target should contain a single [move_count,quadrant] segment"
            );
            assert_eq!(target.segments[0].len(), 2, "zqh segment width must be 2");

            let move_count = target.segments[0][0].round() as usize;
            let quadrant = target.segments[0][1].round() as usize;
            let event_count = scene
                .motion_events
                .iter()
                .filter(|event| event.shape_index == shape_index)
                .count();
            assert!(
                (1..=event_count).contains(&move_count),
                "zqh move count must be within available per-shape events"
            );
            assert!(quadrant < 4, "quadrant index must be in [0,3]");
        }
    }

    #[test]
    fn scene_targets_for_index_are_deterministic() {
        let config = bootstrap_config();
        let first = generate_scene_targets_for_index(&config, 3, 24)
            .expect("target-only generation should succeed");
        let second = generate_scene_targets_for_index(&config, 3, 24)
            .expect("target-only generation should succeed");
        assert_eq!(first, second);
    }

    #[test]
    fn scene_targets_for_index_reject_zero_samples() {
        let config = bootstrap_config();
        let error = generate_scene_targets_for_index(&config, 0, 0)
            .expect_err("zero samples_per_event should fail");
        assert!(matches!(
            error,
            SceneTargetGenerationError::InvalidSamplesPerEvent
        ));
    }
}
