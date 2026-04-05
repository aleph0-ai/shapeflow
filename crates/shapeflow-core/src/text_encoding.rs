use crate::config::SceneConfig;
use crate::scene_generation::SceneGenerationOutput;
use crate::text_semantics::{
    TextAlterationProfile, TextSemanticsError, decode_scene_text_semantics,
    generate_scene_text_lines_with_alteration,
    generate_scene_text_lines_with_scene_config as generate_scene_text_lines_with_scene_config_semantics,
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

pub fn generate_scene_text_lines_with_scene_config(
    scene: &SceneGenerationOutput,
    scene_cfg: &SceneConfig,
) -> Result<Vec<String>, TextEncodingError> {
    generate_scene_text_lines_with_scene_config_semantics(scene, scene_cfg)
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
    use crate::config::TextReferenceFrame;
    use crate::scene_generation::{
        MotionEvent, MotionEventAccounting, SceneGenerationParams, SceneProjectionMode,
        SceneShapePath, generate_scene,
    };
    use crate::seed_schedule::SceneSeedSchedule;
    use crate::tabular_encoding::shape_identity_for_scene;
    use crate::text_semantics::{TextAlterationProfile, derive_scene_text_semantics};
    use crate::{NormalizedPoint, ShapeFlowConfig};

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
            shape_identity_assignment: crate::config::ShapeIdentityAssignment::IndexLocked,
            shape_paths,
            motion_events,
            accounting: MotionEventAccounting {
                expected_total: total_events,
                expected_slots: total_events,
                generated_total: total_events,
                expected_per_shape: per_shape_events.clone(),
                generated_per_shape: per_shape_events,
            },
        }
    }

    fn synthetic_simultaneous_pair_scene() -> SceneGenerationOutput {
        let shape_paths = vec![
            SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![
                    NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                    NormalizedPoint::new(0.2, 0.0).expect("point must build"),
                ],
                soft_memberships: None,
            },
            SceneShapePath {
                shape_index: 1,
                trajectory_points: vec![
                    NormalizedPoint::new(0.1, -0.2).expect("point must build"),
                    NormalizedPoint::new(0.3, 0.2).expect("point must build"),
                ],
                soft_memberships: None,
            },
        ];

        let motion_events = vec![
            MotionEvent {
                global_event_index: 0,
                time_slot: 0,
                shape_index: 0,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                end_point: NormalizedPoint::new(0.1, 0.1).expect("point must build"),
                duration_frames: 24,
                easing: crate::config::EasingFamily::Linear,
            },
            MotionEvent {
                global_event_index: 1,
                time_slot: 0,
                shape_index: 1,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(0.1, -0.2).expect("point must build"),
                end_point: NormalizedPoint::new(0.2, 0.0).expect("point must build"),
                duration_frames: 24,
                easing: crate::config::EasingFamily::Linear,
            },
        ];

        SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(7, 0),
            shape_identity_assignment: crate::config::ShapeIdentityAssignment::IndexLocked,
            shape_paths,
            motion_events,
            accounting: MotionEventAccounting {
                expected_total: 2,
                expected_slots: 2,
                generated_total: 2,
                expected_per_shape: vec![1, 1],
                generated_per_shape: vec![1, 1],
            },
        }
    }

    fn assert_event_metadata_equivalent(
        decoded: &[crate::EventSemanticFrame],
        derived: &[crate::EventSemanticFrame],
    ) {
        assert_eq!(decoded.len(), derived.len());
        for (decoded_event, derived_event) in decoded.iter().zip(derived.iter()) {
            assert_eq!(decoded_event.event_index, derived_event.event_index);
            assert_eq!(decoded_event.shape_id, derived_event.shape_id);
            let mut decoded_simultaneous = decoded_event
                .simultaneous_with
                .iter()
                .map(|peer| peer.event_index)
                .collect::<Vec<_>>();
            decoded_simultaneous.sort_unstable();
            let mut derived_simultaneous = derived_event
                .simultaneous_with
                .iter()
                .map(|peer| peer.event_index)
                .collect::<Vec<_>>();
            derived_simultaneous.sort_unstable();
            assert_eq!(decoded_simultaneous, derived_simultaneous);
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
        let mut config = bootstrap_config();
        config.scene.allow_simultaneous = true;
        config.scene.randomize_motion_events_per_shape = false;
        config.scene.motion_events_per_shape_random_ranges = None;
        config.scene.n_motion_slots = 12;
        config.scene.motion_events_per_shape = vec![4, 4, 4];
        config.scene.n_motion_events_total = Some(12);
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 0,
            samples_per_event: 8,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let lines = generate_scene_text_lines(&scene).expect("text generation should work");

        assert!(
            lines.iter().any(|line| line.contains("same time as Event")),
            "bootstrap fixture should include at least one simultaneous event phrase"
        );
    }

    #[test]
    fn canonical_text_lines_roundtrip_and_layout() {
        let scene = synthetic_scene(4, 2);
        let lines = generate_scene_text_lines(&scene).expect("text generation should work");
        let decoded = decode_scene_text_semantics(&lines).expect("canonical lines should decode");
        let expected =
            derive_scene_text_semantics(&scene).expect("semantic derivation should work");
        assert_eq!(decoded.scene_index, expected.scene_index);
        assert_event_metadata_equivalent(&decoded.events, &expected.events);
        assert_eq!(decoded.pairs, expected.pairs);

        assert_eq!(
            lines.len(),
            1 + expected.events.len() + expected.pairs.len(),
            "line count should match event and pair line total"
        );

        let event_start = 1;
        for (offset, event) in expected.events.iter().enumerate() {
            assert!(
                lines[event_start + offset]
                    .starts_with(&format!("Event {:04}:", event.event_index)),
                "event line {} missing expected prefix",
                event.event_index
            );
            assert!(
                !lines[event_start + offset].contains(" over "),
                "event line {} unexpectedly contained duration phrase",
                event.event_index
            );
            assert!(
                !lines[event_start + offset].contains(" using "),
                "event line {} unexpectedly contained easing phrase",
                event.event_index
            );
            assert!(
                lines[event_start + offset].contains("quadrant"),
                "event line {} should include quadrant wording",
                event.event_index
            );
        }

        let pair_start = 1 + expected.events.len();
        for pair in &expected.pairs {
            assert!(
                lines[pair_start + pair.pair_index]
                    .contains(&format!("Pair {:04}:", pair.pair_index)),
                "pair line {} missing pair marker",
                pair.pair_index
            );
            assert!(
                lines[pair_start + pair.pair_index]
                    .contains(&format!("[event {:04}]", pair.event_index)),
                "pair line {} missing event marker",
                pair.pair_index
            );
            assert!(
                lines[pair_start + pair.pair_index].contains(&pair.first_shape_id),
                "pair line {} missing first shape id",
                pair.pair_index
            );
            assert!(
                lines[pair_start + pair.pair_index].contains(&pair.second_shape_id),
                "pair line {} missing second shape id",
                pair.pair_index
            );
        }
    }

    #[test]
    fn canonical_lines_use_explicit_simultaneous_event_context() {
        let scene = synthetic_simultaneous_pair_scene();
        let lines = generate_scene_text_lines(&scene).expect("text generation should work");
        let semantics =
            derive_scene_text_semantics(&scene).expect("semantic derivation should work");

        let mut saw_simultaneous = false;
        for event in &semantics.events {
            let marker = format!("Event {:04}:", event.event_index);
            let event_line = lines
                .iter()
                .find(|line| line.starts_with(&marker))
                .expect("event line should be present");
            if event.simultaneous_with.is_empty() {
                continue;
            }
            saw_simultaneous = true;
            assert!(
                event_line.contains("same time as Event"),
                "event {} line missing simultaneous context",
                event.event_index
            );
            let mut ids = event
                .simultaneous_with
                .iter()
                .map(|peer| format!("{:04}", peer.event_index))
                .collect::<Vec<_>>();
            ids.sort_unstable();
            for id in ids {
                assert!(
                    event_line.contains(&format!("Event {id}")),
                    "event {} line missing simultaneous event id {}",
                    event.event_index,
                    id
                );
            }
        }
        assert!(
            saw_simultaneous,
            "expected at least one simultaneous event in fixture"
        );
    }

    #[test]
    fn canonical_pair_lines_include_explicit_reference_shape_in_relations() {
        let scene = synthetic_scene(3, 1);
        let lines = generate_scene_text_lines(&scene).expect("text generation should work");
        let semantics =
            derive_scene_text_semantics(&scene).expect("semantic derivation should work");

        let pair_start = 1 + semantics.events.len();
        for pair in &semantics.pairs {
            let line = &lines[pair_start + pair.pair_index];
            let marker = format!(
                "Pair {:04}: [event {:04}] ",
                pair.pair_index, pair.event_index
            );
            assert!(line.starts_with(&marker));
            assert!(
                line.contains(&format!("{} is ", pair.first_shape_id)),
                "pair line {} missing first shape as subject",
                pair.pair_index
            );
            assert!(
                line.contains(" and is "),
                "pair line {} missing two explicit relation clauses",
                pair.pair_index
            );
            let second_id_count = line.matches(&pair.second_shape_id).count();
            assert!(
                second_id_count >= 2,
                "pair {} line should mention second shape in both relation clauses",
                pair.pair_index
            );
        }
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
                        shape_identity_for_scene(&scene, shape_index)
                            .expect("identity must resolve")
                            .shape_id
                    })
                    .collect();

                let pair_lines_start = 1 + scene.motion_events.len();
                let mut expected_pair_index = 0usize;
                for event in &scene.motion_events {
                    for i in 0..shape_count {
                        for j in (i + 1)..shape_count {
                            let expected_pair_marker = format!("Pair {:04}:", expected_pair_index);
                            let expected_event_marker =
                                format!("[event {:04}] ", event.global_event_index);
                            let expected_pair_line = &lines[pair_lines_start + expected_pair_index];
                            assert!(
                                expected_pair_line.starts_with(&expected_pair_marker),
                                "pair index mismatch at line for event {} pair ({i}, {j}); expected marker {expected_pair_marker}",
                                event.global_event_index
                            );
                            assert!(
                                expected_pair_line.contains(&expected_event_marker),
                                "missing event marker {expected_event_marker} for pair ({i}, {j})"
                            );

                            let first_id = &identities[i];
                            let second_id = &identities[j];
                            assert!(
                                expected_pair_line.contains(first_id),
                                "missing first shape id {first_id} for event {} pair ({i}, {j})",
                                event.global_event_index
                            );
                            assert!(
                                expected_pair_line.contains(second_id),
                                "missing second shape id {second_id} for event {} pair ({i}, {j})",
                                event.global_event_index
                            );
                            let first_position = expected_pair_line
                                .find(first_id)
                                .expect("generated pair line should include first shape id");
                            let second_position = expected_pair_line
                                .find(second_id)
                                .expect("generated pair line should include second shape id");
                            assert!(
                                first_position < second_position,
                                "pair line ordering changed for event {} pair ({i}, {j})",
                                event.global_event_index
                            );

                            assert!(
                                lines.iter().any(|line| {
                                    line.starts_with("Pair ")
                                        && line.contains(&expected_event_marker)
                                        && line.contains(first_id)
                                        && line.contains(second_id)
                                }),
                                "missing pair sentence for event {} ({first_id}, {second_id})",
                                event.global_event_index
                            );

                            expected_pair_index += 1;
                        }
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
    fn generation_with_non_default_text_config_is_deterministic() {
        let mut config = bootstrap_config();
        config.scene.text_reference_frame = TextReferenceFrame::Mixed;
        config.scene.text_synonym_rate = 0.4;
        config.scene.text_typo_rate = 0.12;

        let params = SceneGenerationParams {
            config: &config,
            scene_index: 5,
            samples_per_event: 16,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let first = generate_scene_text_lines_with_scene_config(&scene, &config.scene)
            .expect("scene text should generate");
        let second = generate_scene_text_lines_with_scene_config(&scene, &config.scene)
            .expect("scene text should generate");
        assert_eq!(first, second);
    }

    #[test]
    fn generation_with_non_default_text_config_decodes_to_same_semantics() {
        let mut config = bootstrap_config();
        config.scene.text_reference_frame = TextReferenceFrame::Relative;
        config.scene.text_synonym_rate = 0.3;
        config.scene.text_typo_rate = 0.25;

        let params = SceneGenerationParams {
            config: &config,
            scene_index: 4,
            samples_per_event: 10,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let canonical =
            generate_scene_text_lines(&scene).expect("canonical generation should succeed");
        let expected =
            decode_scene_text_semantics(&canonical).expect("canonical decode should work");
        let varied = generate_scene_text_lines_with_scene_config(&scene, &config.scene)
            .expect("configured generation should succeed");
        let decoded = decode_scene_text_semantics(&varied).expect("configured decode should work");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn generation_with_non_default_text_config_preserves_line_count_and_order() {
        let mut config = bootstrap_config();
        config.scene.text_reference_frame = TextReferenceFrame::Mixed;
        config.scene.text_synonym_rate = 0.5;
        config.scene.text_typo_rate = 0.0;

        let params = SceneGenerationParams {
            config: &config,
            scene_index: 2,
            samples_per_event: 12,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let lines = generate_scene_text_lines_with_scene_config(&scene, &config.scene)
            .expect("scene text should generate");
        let semantics =
            derive_scene_text_semantics(&scene).expect("semantic derivation should work");

        assert_eq!(
            lines.len(),
            1 + semantics.events.len() + semantics.pairs.len()
        );

        for (offset, event) in semantics.events.iter().enumerate() {
            let marker = format!("Event {:04}:", event.event_index);
            assert!(
                lines[1 + offset].contains(&marker),
                "event line {} lost marker {}",
                event.event_index,
                marker
            );
        }

        let pair_start = 1 + semantics.events.len();
        for (offset, pair) in semantics.pairs.iter().enumerate() {
            let marker = format!("Pair {:04}:", pair.pair_index);
            assert!(
                lines[pair_start + offset].contains(&marker),
                "pair line {} lost marker {}",
                pair.pair_index,
                marker
            );
            assert!(
                lines[pair_start + offset].contains(&pair.event_index.to_string()),
                "pair line {} missing event marker for event {}",
                pair.pair_index,
                pair.event_index
            );
            assert!(
                lines[pair_start + offset].contains(&pair.first_shape_id),
                "pair line {} missing first shape id {}",
                pair.pair_index,
                pair.first_shape_id
            );
            assert!(
                lines[pair_start + offset].contains(&pair.second_shape_id),
                "pair line {} missing second shape id {}",
                pair.pair_index,
                pair.second_shape_id
            );
        }
    }

    #[test]
    fn generation_fails_on_invalid_shape_index() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_identity_assignment: crate::config::ShapeIdentityAssignment::IndexLocked,
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
                expected_slots: 1,
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
            shape_identity_assignment: crate::config::ShapeIdentityAssignment::IndexLocked,
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
                expected_slots: 1,
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
                let expected_shape_id = shape_identity_for_scene(&scene, 0)
                    .expect("shape identity should resolve")
                    .shape_id;
                assert_eq!(shape_id, expected_shape_id);
                assert_eq!(shape_index, 0);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}
