use crate::landscape::SoftQuadrantMembership;
use crate::scene_generation::SceneGenerationOutput;

const SIMPLEX_SUM_TOLERANCE: f64 = 1.0e-9;
const INTEGER_COMPONENT_TOLERANCE: f64 = 1.0e-12;

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
}
