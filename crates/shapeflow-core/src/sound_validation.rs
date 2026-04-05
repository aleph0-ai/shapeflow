use std::collections::BTreeMap;
use std::io::Cursor;

use crate::config::SoundChannelMapping;
use crate::scene_generation::SceneGenerationOutput;
use crate::sound_encoding::{SoundEncodingError, render_scene_sound_wav};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundValidationReport {
    pub channel_count: u16,
    pub sample_rate_hz: u32,
    pub interleaved_sample_count: usize,
    pub samples_per_channel: usize,
    pub expected_samples_per_channel: usize,
    pub wav_byte_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SoundValidationError {
    #[error("sound rendering failed: {0}")]
    Rendering(#[from] SoundEncodingError),
    #[error("scene has no motion events")]
    EmptyScene,
    #[error("wav decoding failed: {0}")]
    WavDecoding(String),
    #[error("wav channel count mismatch: expected {expected}, found {found}")]
    ChannelCountMismatch { expected: u16, found: u16 },
    #[error("wav sample rate mismatch: expected {expected}, found {found}")]
    SampleRateMismatch { expected: u32, found: u32 },
    #[error("wav bits per sample mismatch: expected 16, found {found}")]
    BitsPerSampleMismatch { found: u16 },
    #[error("wav sample format mismatch: expected PCM int")]
    SampleFormatMismatch,
    #[error(
        "wav sample payload length {sample_count} is not divisible by channel count {channel_count}"
    )]
    InterleavedSampleCountNotDivisible {
        sample_count: usize,
        channel_count: u16,
    },
    #[error("wav sample payload is empty")]
    EmptySamplePayload,
    #[error(
        "sample count mismatch: expected {expected_samples_per_channel} per channel, found {found_samples_per_channel}"
    )]
    SampleCountMismatch {
        expected_samples_per_channel: usize,
        found_samples_per_channel: usize,
    },
    #[error("expected sample count overflow while summing slot durations")]
    ExpectedSampleCountOverflow,
}

pub fn validate_scene_sound_wav(
    scene: &SceneGenerationOutput,
    sample_rate_hz: u32,
    frames_per_second: u16,
    modulation_depth_per_mille: u16,
    channel_mapping: SoundChannelMapping,
) -> Result<SoundValidationReport, SoundValidationError> {
    if scene.motion_events.is_empty() {
        return Err(SoundValidationError::EmptyScene);
    }

    let expected_samples_per_channel =
        expected_samples_per_channel(scene, sample_rate_hz, frames_per_second)?;
    let wav_bytes = render_scene_sound_wav(
        scene,
        sample_rate_hz,
        frames_per_second,
        modulation_depth_per_mille,
        channel_mapping,
    )?;

    let expected_channel_count = channel_count(channel_mapping);
    let mut reader = hound::WavReader::new(Cursor::new(&wav_bytes))
        .map_err(|error| SoundValidationError::WavDecoding(error.to_string()))?;
    let spec = reader.spec();
    if spec.channels != expected_channel_count {
        return Err(SoundValidationError::ChannelCountMismatch {
            expected: expected_channel_count,
            found: spec.channels,
        });
    }
    if spec.sample_rate != sample_rate_hz {
        return Err(SoundValidationError::SampleRateMismatch {
            expected: sample_rate_hz,
            found: spec.sample_rate,
        });
    }
    if spec.bits_per_sample != 16 {
        return Err(SoundValidationError::BitsPerSampleMismatch {
            found: spec.bits_per_sample,
        });
    }
    if spec.sample_format != hound::SampleFormat::Int {
        return Err(SoundValidationError::SampleFormatMismatch);
    }

    let interleaved_sample_count =
        reader
            .samples::<i16>()
            .try_fold(0usize, |count, sample_result| {
                sample_result
                    .map(|_| count + 1)
                    .map_err(|error| SoundValidationError::WavDecoding(error.to_string()))
            })?;
    if interleaved_sample_count == 0 {
        return Err(SoundValidationError::EmptySamplePayload);
    }
    if interleaved_sample_count % usize::from(spec.channels) != 0 {
        return Err(SoundValidationError::InterleavedSampleCountNotDivisible {
            sample_count: interleaved_sample_count,
            channel_count: spec.channels,
        });
    }

    let samples_per_channel = interleaved_sample_count / usize::from(spec.channels);
    if samples_per_channel != expected_samples_per_channel {
        return Err(SoundValidationError::SampleCountMismatch {
            expected_samples_per_channel,
            found_samples_per_channel: samples_per_channel,
        });
    }

    Ok(SoundValidationReport {
        channel_count: spec.channels,
        sample_rate_hz: spec.sample_rate,
        interleaved_sample_count,
        samples_per_channel,
        expected_samples_per_channel,
        wav_byte_count: wav_bytes.len(),
    })
}

fn expected_samples_per_channel(
    scene: &SceneGenerationOutput,
    sample_rate_hz: u32,
    frames_per_second: u16,
) -> Result<usize, SoundValidationError> {
    let shape_count = scene.shape_paths.len();
    let default_duration_frames = scene
        .motion_events
        .iter()
        .min_by_key(|event| event.time_slot)
        .map(|event| event.duration_frames)
        .ok_or(SoundValidationError::EmptyScene)?;
    let mut durations_by_slot = (0..scene.accounting.expected_slots)
        .map(|slot| (slot, None))
        .collect::<BTreeMap<u32, Option<u16>>>();
    for event in &scene.motion_events {
        if event.shape_index >= shape_count {
            return Err(SoundValidationError::Rendering(
                SoundEncodingError::ShapeIndexOutOfBounds {
                    shape_index: event.shape_index,
                    shape_count,
                },
            ));
        }
        if event.duration_frames == 0 {
            return Err(SoundValidationError::Rendering(
                SoundEncodingError::ZeroEventDuration {
                    global_event_index: event.global_event_index,
                },
            ));
        }

        match durations_by_slot.get_mut(&event.time_slot) {
            Some(expected_duration) => {
                if let Some(expected) = *expected_duration {
                    if expected != event.duration_frames {
                        return Err(SoundValidationError::Rendering(
                            SoundEncodingError::MismatchedSlotDuration {
                                time_slot: event.time_slot,
                                expected,
                                found: event.duration_frames,
                                global_event_index: event.global_event_index,
                            },
                        ));
                    }
                } else {
                    *expected_duration = Some(event.duration_frames);
                }
            }
            None => {
                return Err(SoundValidationError::Rendering(
                    SoundEncodingError::WavEncoding(format!(
                        "event time_slot {} exceeds declared slot count {}",
                        event.time_slot, scene.accounting.expected_slots
                    )),
                ));
            }
        }
    }

    let mut total_samples = 0usize;
    for duration_frames in durations_by_slot.values() {
        let duration_frames = duration_frames.unwrap_or(default_duration_frames);
        let slot_samples = usize::try_from(
            (u128::from(duration_frames) * u128::from(sample_rate_hz)
                + (u128::from(frames_per_second) - 1))
                / u128::from(frames_per_second),
        )
        .map_err(|_| SoundValidationError::ExpectedSampleCountOverflow)?;
        total_samples = total_samples
            .checked_add(slot_samples)
            .ok_or(SoundValidationError::ExpectedSampleCountOverflow)?;
    }
    Ok(total_samples)
}

fn channel_count(channel_mapping: SoundChannelMapping) -> u16 {
    match channel_mapping {
        SoundChannelMapping::MonoMix => 1,
        SoundChannelMapping::StereoAlternating => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EasingFamily, ShapeIdentityAssignment};
    use crate::scene_generation::{
        MotionEvent, MotionEventAccounting, SceneGenerationOutput, SceneGenerationParams,
        SceneProjectionMode, SceneShapePath, generate_scene,
    };
    use crate::seed_schedule::SceneSeedSchedule;
    use crate::{NormalizedPoint, ShapeFlowConfig, SoundChannelMapping};

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config must parse")
    }

    fn point(x: f64, y: f64) -> NormalizedPoint {
        NormalizedPoint::new(x, y).expect("point must be normalized")
    }

    fn rounded_slot_samples(
        duration_frames: u16,
        sample_rate_hz: u32,
        frames_per_second: u16,
    ) -> usize {
        usize::try_from(
            (u128::from(duration_frames) * u128::from(sample_rate_hz)
                + (u128::from(frames_per_second) - 1))
                / u128::from(frames_per_second),
        )
        .expect("test sample count must fit usize")
    }

    fn synthetic_scene(
        shape_count: usize,
        motion_events: Vec<MotionEvent>,
    ) -> SceneGenerationOutput {
        let mut generated_per_shape = vec![0u16; shape_count];
        for event in &motion_events {
            generated_per_shape[event.shape_index] = generated_per_shape[event.shape_index]
                .checked_add(1)
                .expect("per-shape count should fit u16");
        }
        let expected_total =
            u32::try_from(motion_events.len()).expect("motion-event length should fit u32");
        let expected_slots = motion_events
            .iter()
            .map(|event| event.time_slot)
            .max()
            .unwrap_or(0)
            + 1;

        SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(7, 0),
            shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
            shape_paths: (0..shape_count)
                .map(|shape_index| SceneShapePath {
                    shape_index,
                    trajectory_points: vec![point(-0.2, -0.1), point(0.2, 0.3)],
                    soft_memberships: None,
                })
                .collect(),
            motion_events,
            accounting: MotionEventAccounting {
                expected_total,
                expected_slots,
                generated_total: expected_total,
                expected_per_shape: generated_per_shape.clone(),
                generated_per_shape,
            },
        }
    }

    #[test]
    fn validate_scene_sound_wav_reports_expected_metrics_for_bootstrap_scene() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 0,
            samples_per_event: 24,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let report = validate_scene_sound_wav(
            &scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            config.scene.sound_channel_mapping,
        )
        .expect("sound validation should succeed");

        assert_eq!(
            report.channel_count,
            channel_count(config.scene.sound_channel_mapping)
        );
        assert_eq!(report.sample_rate_hz, config.scene.sound_sample_rate_hz);
        assert_eq!(
            report.samples_per_channel,
            report.expected_samples_per_channel
        );
        assert_eq!(
            report.interleaved_sample_count % usize::from(report.channel_count),
            0
        );
        assert!(report.wav_byte_count > 0);
    }

    #[test]
    fn validate_scene_sound_wav_rejects_empty_scene() {
        let config = bootstrap_config();
        let mut scene = {
            let params = SceneGenerationParams {
                config: &config,
                scene_index: 0,
                samples_per_event: 24,
                projection: SceneProjectionMode::TrajectoryOnly,
            };
            generate_scene(&params).expect("scene generation should succeed")
        };
        scene.motion_events.clear();

        let err = validate_scene_sound_wav(
            &scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::StereoAlternating,
        )
        .expect_err("empty scene should fail sound validation");
        assert!(matches!(err, SoundValidationError::EmptyScene));
    }

    #[test]
    fn expected_samples_per_channel_sums_unique_slot_durations() {
        let sample_rate_hz = 1_000;
        let frames_per_second = 24;
        let scene = synthetic_scene(
            2,
            vec![
                MotionEvent {
                    global_event_index: 0,
                    time_slot: 0,
                    shape_index: 0,
                    shape_event_index: 0,
                    start_point: point(-0.8, -0.8),
                    end_point: point(-0.2, -0.2),
                    duration_frames: 3,
                    easing: EasingFamily::Linear,
                },
                MotionEvent {
                    global_event_index: 1,
                    time_slot: 0,
                    shape_index: 1,
                    shape_event_index: 0,
                    start_point: point(0.8, 0.8),
                    end_point: point(0.2, 0.2),
                    duration_frames: 3,
                    easing: EasingFamily::EaseInOut,
                },
                MotionEvent {
                    global_event_index: 2,
                    time_slot: 1,
                    shape_index: 0,
                    shape_event_index: 1,
                    start_point: point(-0.2, -0.2),
                    end_point: point(0.0, 0.4),
                    duration_frames: 5,
                    easing: EasingFamily::EaseIn,
                },
                MotionEvent {
                    global_event_index: 3,
                    time_slot: 1,
                    shape_index: 1,
                    shape_event_index: 1,
                    start_point: point(0.2, 0.2),
                    end_point: point(0.0, -0.4),
                    duration_frames: 5,
                    easing: EasingFamily::EaseOut,
                },
                MotionEvent {
                    global_event_index: 4,
                    time_slot: 2,
                    shape_index: 0,
                    shape_event_index: 2,
                    start_point: point(0.0, 0.4),
                    end_point: point(0.4, 0.6),
                    duration_frames: 2,
                    easing: EasingFamily::Linear,
                },
            ],
        );

        let expected = rounded_slot_samples(3, sample_rate_hz, frames_per_second)
            + rounded_slot_samples(5, sample_rate_hz, frames_per_second)
            + rounded_slot_samples(2, sample_rate_hz, frames_per_second);
        let found = expected_samples_per_channel(&scene, sample_rate_hz, frames_per_second)
            .expect("slot durations are valid");
        assert_eq!(found, expected);
    }

    #[test]
    fn expected_samples_per_channel_is_order_insensitive_for_consistent_slots() {
        let sample_rate_hz = 44_100;
        let frames_per_second = 24;
        let ordered = synthetic_scene(
            2,
            vec![
                MotionEvent {
                    global_event_index: 0,
                    time_slot: 0,
                    shape_index: 0,
                    shape_event_index: 0,
                    start_point: point(-0.4, -0.4),
                    end_point: point(-0.2, -0.2),
                    duration_frames: 6,
                    easing: EasingFamily::Linear,
                },
                MotionEvent {
                    global_event_index: 1,
                    time_slot: 0,
                    shape_index: 1,
                    shape_event_index: 0,
                    start_point: point(0.4, 0.4),
                    end_point: point(0.2, 0.2),
                    duration_frames: 6,
                    easing: EasingFamily::EaseInOut,
                },
                MotionEvent {
                    global_event_index: 2,
                    time_slot: 2,
                    shape_index: 1,
                    shape_event_index: 1,
                    start_point: point(0.2, 0.2),
                    end_point: point(0.0, -0.2),
                    duration_frames: 4,
                    easing: EasingFamily::EaseOut,
                },
            ],
        );
        let reordered = synthetic_scene(
            2,
            vec![
                ordered.motion_events[2],
                ordered.motion_events[0],
                ordered.motion_events[1],
            ],
        );

        let ordered_count =
            expected_samples_per_channel(&ordered, sample_rate_hz, frames_per_second)
                .expect("ordered scene should validate");
        let reordered_count =
            expected_samples_per_channel(&reordered, sample_rate_hz, frames_per_second)
                .expect("reordered scene should validate");
        assert_eq!(ordered_count, reordered_count);
    }

    #[test]
    fn expected_samples_per_channel_rejects_mismatched_slot_duration() {
        let scene = synthetic_scene(
            2,
            vec![
                MotionEvent {
                    global_event_index: 0,
                    time_slot: 3,
                    shape_index: 0,
                    shape_event_index: 0,
                    start_point: point(-0.1, -0.1),
                    end_point: point(0.1, 0.1),
                    duration_frames: 4,
                    easing: EasingFamily::Linear,
                },
                MotionEvent {
                    global_event_index: 1,
                    time_slot: 3,
                    shape_index: 1,
                    shape_event_index: 0,
                    start_point: point(0.1, 0.1),
                    end_point: point(-0.1, -0.1),
                    duration_frames: 5,
                    easing: EasingFamily::Linear,
                },
            ],
        );

        let err = expected_samples_per_channel(&scene, 16_000, 30)
            .expect_err("mismatched slot durations should fail");
        assert!(matches!(
            err,
            SoundValidationError::Rendering(SoundEncodingError::MismatchedSlotDuration {
                time_slot: 3,
                expected: 4,
                found: 5,
                global_event_index: 1,
            })
        ));
    }
}
