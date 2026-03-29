use crate::scene_generation::SceneGenerationOutput;
use crate::text_semantics::{
    TextAlterationProfile, TextSemanticsError, decode_scene_text_semantics,
    generate_scene_text_lines_with_alteration,
};

pub use crate::text_semantics::TextSemanticsError as TextEncodingError;

pub fn generate_scene_text_lines(
    scene: &SceneGenerationOutput,
) -> Result<Vec<String>, TextEncodingError> {
    generate_scene_text_lines_with_profile(scene, TextAlterationProfile::Canonical)
}

pub fn generate_scene_text_lines_with_profile(
    scene: &SceneGenerationOutput,
    profile: TextAlterationProfile,
) -> Result<Vec<String>, TextEncodingError> {
    generate_scene_text_lines_with_alteration(scene, profile)
}

pub fn assert_text_lines_match_semantics(
    scene: &SceneGenerationOutput,
    lines: &[String],
) -> Result<(), TextEncodingError> {
    let canonical_lines =
        generate_scene_text_lines_with_alteration(scene, TextAlterationProfile::Canonical)?;
    let expected = decode_scene_text_semantics(&canonical_lines)?;
    let decoded = decode_scene_text_semantics(lines)?;
    if decoded != expected {
        return Err(TextSemanticsError::ParseLine {
            line: "decoded semantics mismatch for generated scene text".to_string(),
        });
    }
    Ok(())
}

pub fn serialize_scene_text(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_generation::{
        MotionEvent, MotionEventAccounting, SceneGenerationParams, SceneProjectionMode,
        SceneShapePath, generate_scene,
    };
    use crate::seed_schedule::SceneSeedSchedule;
    use crate::text_semantics::{TextAlterationProfile, derive_scene_text_semantics};
    use crate::{NormalizedPoint, ShapeFlowConfig, shape_identity_for_index};

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config must parse")
    }

    fn synthetic_scene(shape_count: usize, events_per_shape: usize) -> SceneGenerationOutput {
        let mut shape_paths = Vec::with_capacity(shape_count);
        for shape_index in 0..shape_count {
            let base = shape_index as f64 * 0.13;
            shape_paths.push(SceneShapePath {
                shape_index,
                trajectory_points: vec![
                    NormalizedPoint::new(base, -base).expect("point must build"),
                    NormalizedPoint::new(base + 0.01, -base + 0.02).expect("point must build"),
                ],
                soft_memberships: None,
            });
        }

        let mut motion_events = Vec::with_capacity(shape_count * events_per_shape);
        let mut global_event_index: u32 = 0;
        for shape_index in 0..shape_count {
            for shape_event_index in 0..events_per_shape {
                let shape_x = shape_index as f64 * 0.2;
                let shape_y = shape_index as f64 * 0.15;
                motion_events.push(MotionEvent {
                    global_event_index,
                    time_slot: global_event_index,
                    shape_index,
                    shape_event_index: shape_event_index as u16,
                    start_point: NormalizedPoint::new(shape_x, shape_y).expect("point must build"),
                    end_point: NormalizedPoint::new(shape_x + 0.05, shape_y + 0.03)
                        .expect("point must build"),
                    duration_frames: 24,
                    easing: crate::config::EasingFamily::Linear,
                });
                global_event_index += 1;
            }
        }

        let per_shape_events = vec![events_per_shape as u16; shape_count];
        let total_events = (shape_count * events_per_shape) as u32;
        SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(7, 0),
            shape_paths,
            motion_events,
            accounting: MotionEventAccounting {
                expected_total: total_events,
                generated_total: total_events,
                expected_per_shape: per_shape_events.clone(),
                generated_per_shape: per_shape_events,
            },
        }
    }

    #[test]
    fn generated_text_lines_are_deterministic() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 1,
            samples_per_event: 12,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let first = generate_scene_text_lines(&scene).expect("text generation should work");
        let second = generate_scene_text_lines(&scene).expect("text generation should work");
        assert_eq!(first, second);
        assert_text_lines_match_semantics(&scene, &first)
            .expect("generated text should decode to derived semantics");
        let serialized = serialize_scene_text(&first);
        assert!(serialized.contains("Scene 00000000000000000000000000000001"));
    }

    #[test]
    fn text_mentions_simultaneous_shapes_for_shared_time_slots() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 0,
            samples_per_event: 8,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let lines = generate_scene_text_lines(&scene).expect("text generation should work");

        assert!(
            lines
                .iter()
                .any(|line| line.contains("while simultaneous with")),
            "bootstrap fixture should include at least one simultaneous event phrase"
        );
    }

    #[test]
    fn text_mentions_every_motion_event_and_shape_pair() {
        for shape_count in 2..=5 {
            for events_per_shape in 1..=4 {
                let scene = synthetic_scene(shape_count, events_per_shape);
                let lines = generate_scene_text_lines(&scene).expect("text generation should work");
                let semantics =
                    derive_scene_text_semantics(&scene).expect("semantic derivation should work");
                let expected_pair_lines = semantics.pairs.len();

                assert_eq!(
                    lines.len(),
                    scene.motion_events.len() + 1 + expected_pair_lines
                );

                for event in &scene.motion_events {
                    let marker = format!("Event {:04}:", event.global_event_index);
                    assert!(
                        lines.iter().any(|line| line.contains(&marker)),
                        "missing event sentence for event {}",
                        event.global_event_index
                    );
                }

                let identities: Vec<_> = (0..shape_count)
                    .map(|shape_index| {
                        shape_identity_for_index(shape_index)
                            .expect("identity must resolve")
                            .shape_id
                    })
                    .collect();

                let pair_lines_start = 1 + scene.motion_events.len();
                let mut expected_pair_index = 0usize;
                for i in 0..shape_count {
                    for j in (i + 1)..shape_count {
                        let expected_pair_marker = format!("Pair {:04}:", expected_pair_index);
                        let expected_pair_line = &lines[pair_lines_start + expected_pair_index];
                        assert!(
                            expected_pair_line.starts_with(&expected_pair_marker),
                            "pair index mismatch at line for pair ({i}, {j}); expected marker {expected_pair_marker}"
                        );

                        let first_id = &identities[i];
                        let second_id = &identities[j];
                        assert!(
                            expected_pair_line.contains(first_id),
                            "missing first shape id {first_id} for pair ({i}, {j})"
                        );
                        assert!(
                            expected_pair_line.contains(second_id),
                            "missing second shape id {second_id} for pair ({i}, {j})"
                        );
                        let first_position = expected_pair_line
                            .find(first_id)
                            .expect("generated pair line should include first shape id");
                        let second_position = expected_pair_line
                            .find(second_id)
                            .expect("generated pair line should include second shape id");
                        assert!(
                            first_position < second_position,
                            "pair line ordering changed for ({i}, {j})"
                        );

                        assert!(
                            lines.iter().any(|line| {
                                line.starts_with("Pair ")
                                    && line.contains(first_id)
                                    && line.contains(second_id)
                            }),
                            "missing pair sentence for {first_id} and {second_id}"
                        );

                        expected_pair_index += 1;
                    }
                }
                assert_eq!(expected_pair_index, expected_pair_lines);
            }
        }
    }

    #[test]
    fn all_alteration_profiles_decode_to_same_semantics() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 3,
            samples_per_event: 16,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let canonical =
            generate_scene_text_lines_with_profile(&scene, TextAlterationProfile::Canonical)
                .expect("canonical generation should succeed");
        let expected =
            decode_scene_text_semantics(&canonical).expect("canonical lines should decode");
        let reordered =
            generate_scene_text_lines_with_profile(&scene, TextAlterationProfile::FullyReordered)
                .expect("reordered generation should succeed");

        for profile in [
            TextAlterationProfile::Canonical,
            TextAlterationProfile::EventClauseReordered,
            TextAlterationProfile::PairClauseReordered,
            TextAlterationProfile::FullyReordered,
        ] {
            let lines = generate_scene_text_lines_with_profile(&scene, profile)
                .expect("profile generation should succeed");
            let decoded = decode_scene_text_semantics(&lines).expect("profile lines should decode");
            assert_eq!(decoded, expected);
        }

        assert_ne!(
            canonical, reordered,
            "canonical and reordered surfaces should differ"
        );
    }

    #[test]
    fn generation_fails_on_invalid_shape_index() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_paths: vec![SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![NormalizedPoint::new(0.0, 0.0).expect("point must build")],
                soft_memberships: None,
            }],
            motion_events: vec![MotionEvent {
                global_event_index: 0,
                time_slot: 0,
                shape_index: 1,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                end_point: NormalizedPoint::new(0.1, 0.1).expect("point must build"),
                duration_frames: 24,
                easing: crate::config::EasingFamily::Linear,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1],
                generated_per_shape: vec![1],
            },
        };

        let error = generate_scene_text_lines(&scene).expect_err("invalid scene should fail");
        match error {
            TextEncodingError::ShapeIndexOutOfBounds {
                shape_index,
                shape_count,
            } => {
                assert_eq!(shape_index, 1);
                assert_eq!(shape_count, 1);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn generation_fails_when_anchor_point_is_missing() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_paths: vec![
                SceneShapePath {
                    shape_index: 0,
                    trajectory_points: vec![],
                    soft_memberships: None,
                },
                SceneShapePath {
                    shape_index: 1,
                    trajectory_points: vec![
                        NormalizedPoint::new(0.1, 0.1).expect("point must build"),
                    ],
                    soft_memberships: None,
                },
            ],
            motion_events: vec![MotionEvent {
                global_event_index: 0,
                time_slot: 0,
                shape_index: 0,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(0.2, 0.2).expect("point must build"),
                end_point: NormalizedPoint::new(0.3, 0.3).expect("point must build"),
                duration_frames: 24,
                easing: crate::config::EasingFamily::Linear,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1, 0],
                generated_per_shape: vec![1, 0],
            },
        };
        let error = generate_scene_text_lines(&scene).expect_err("scene should fail");
        match error {
            TextEncodingError::MissingAnchorPoint {
                shape_id,
                shape_index,
            } => {
                assert_eq!(shape_id, "circle_red");
                assert_eq!(shape_index, 0);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}
