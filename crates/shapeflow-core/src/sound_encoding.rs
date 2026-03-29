use std::f64::consts::PI;
use std::io::Cursor;

use crate::config::{EasingFamily, SoundChannelMapping};
use crate::scene_generation::SceneGenerationOutput;

#[derive(Debug, thiserror::Error)]
pub enum SoundEncodingError {
    #[error("invalid sample rate: {sample_rate_hz}")]
    InvalidSampleRate { sample_rate_hz: u32 },
    #[error("invalid frames_per_second: {frames_per_second}")]
    InvalidFramesPerSecond { frames_per_second: u16 },
    #[error(
        "invalid modulation_depth_per_mille: must be <= 1000, got {modulation_depth_per_mille}"
    )]
    InvalidModulationDepthPerMille { modulation_depth_per_mille: u16 },
    #[error("shape index {shape_index} out of bounds for scene with {shape_count} shapes")]
    ShapeIndexOutOfBounds {
        shape_index: usize,
        shape_count: usize,
    },
    #[error("motion event {global_event_index} has zero duration")]
    ZeroEventDuration { global_event_index: u32 },
    #[error(
        "time_slot {time_slot} has inconsistent durations: expected {expected}, event {global_event_index} has {found}"
    )]
    MismatchedSlotDuration {
        time_slot: u32,
        expected: u16,
        found: u16,
        global_event_index: u32,
    },
    #[error("wav encoding failed: {0}")]
    WavEncoding(String),
}

pub fn render_scene_sound_wav(
    scene: &SceneGenerationOutput,
    sample_rate_hz: u32,
    frames_per_second: u16,
    modulation_depth_per_mille: u16,
    channel_mapping: SoundChannelMapping,
) -> Result<Vec<u8>, SoundEncodingError> {
    if sample_rate_hz == 0 {
        return Err(SoundEncodingError::InvalidSampleRate { sample_rate_hz });
    }
    if frames_per_second == 0 {
        return Err(SoundEncodingError::InvalidFramesPerSecond { frames_per_second });
    }
    if modulation_depth_per_mille > 1000 {
        return Err(SoundEncodingError::InvalidModulationDepthPerMille {
            modulation_depth_per_mille,
        });
    }
    if scene.motion_events.is_empty() {
        return Ok(Vec::new());
    }

    let shape_count = scene.shape_paths.len();
    let max_time_slot = scene
        .motion_events
        .iter()
        .map(|event| event.time_slot)
        .max()
        .unwrap_or(0);
    let mut events_by_slot = vec![
        Vec::new();
        usize::try_from(max_time_slot).map_err(|_| {
            SoundEncodingError::WavEncoding("time slot index exceeds platform limits".to_string())
        })? + 1
    ];

    for event in &scene.motion_events {
        if event.shape_index >= shape_count {
            return Err(SoundEncodingError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        if event.duration_frames == 0 {
            return Err(SoundEncodingError::ZeroEventDuration {
                global_event_index: event.global_event_index,
            });
        }

        let slot = usize::try_from(event.time_slot).map_err(|_| {
            SoundEncodingError::WavEncoding(
                "time slot index cannot be represented for indexing".to_string(),
            )
        })?;
        events_by_slot[slot].push(event);
    }

    let mut interleaved_samples = Vec::<i16>::new();
    let channel_count: u16 = match channel_mapping {
        SoundChannelMapping::MonoMix => 1,
        SoundChannelMapping::StereoAlternating => 2,
    };
    for (slot_index, slot_events) in events_by_slot.iter_mut().enumerate() {
        if slot_events.is_empty() {
            continue;
        }
        slot_events.sort_by_key(|event| event.global_event_index);
        let expected_duration_frames = slot_events[0].duration_frames;
        for event in slot_events.iter().skip(1) {
            if event.duration_frames != expected_duration_frames {
                return Err(SoundEncodingError::MismatchedSlotDuration {
                    time_slot: slot_index as u32,
                    expected: expected_duration_frames,
                    found: event.duration_frames,
                    global_event_index: event.global_event_index,
                });
            }
        }

        let samples_per_event =
            samples_for_duration(expected_duration_frames, sample_rate_hz, frames_per_second)?;
        let event_gain = (i16::MAX as f64) * 0.35 / (slot_events.len() as f64).max(1.0);
        for sample_index in 0..samples_per_event {
            let t = slot_progress(sample_index, samples_per_event);
            let mut mono = 0.0_f64;
            let mut left = 0.0_f64;
            let mut right = 0.0_f64;

            for event in slot_events.iter() {
                let eased = easing_progress(t, event.easing);
                let x = lerp(event.start_point.x, event.end_point.x, eased);
                let y = lerp(event.start_point.y, event.end_point.y, eased);
                let frequency =
                    modulated_frequency(event.shape_index, x, y, modulation_depth_per_mille);
                let sample_time = sample_index as f64 / sample_rate_hz as f64;
                let wave = (2.0 * PI * frequency * sample_time).sin() * event_gain;

                match channel_mapping {
                    SoundChannelMapping::MonoMix => {
                        mono += wave;
                    }
                    SoundChannelMapping::StereoAlternating => {
                        if event.shape_index % 2 == 0 {
                            left += wave;
                        } else {
                            right += wave;
                        }
                    }
                }
            }

            match channel_mapping {
                SoundChannelMapping::MonoMix => {
                    interleaved_samples.push(clamp_i16(mono));
                }
                SoundChannelMapping::StereoAlternating => {
                    interleaved_samples.push(clamp_i16(left));
                    interleaved_samples.push(clamp_i16(right));
                }
            }
        }
    }

    let spec = hound::WavSpec {
        channels: channel_count,
        sample_rate: sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|error| SoundEncodingError::WavEncoding(error.to_string()))?;
        for sample in interleaved_samples {
            writer
                .write_sample(sample)
                .map_err(|error| SoundEncodingError::WavEncoding(error.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|error| SoundEncodingError::WavEncoding(error.to_string()))?;
    }

    Ok(cursor.into_inner())
}

fn samples_for_duration(
    duration_frames: u16,
    sample_rate_hz: u32,
    frames_per_second: u16,
) -> Result<usize, SoundEncodingError> {
    let sample_count = (u128::from(duration_frames) * u128::from(sample_rate_hz)
        + (u128::from(frames_per_second) - 1))
        / u128::from(frames_per_second);
    usize::try_from(sample_count).map_err(|_| {
        SoundEncodingError::WavEncoding(
            "computed sample count exceeds addressable size".to_string(),
        )
    })
}

fn slot_progress(sample_index: usize, sample_count: usize) -> f64 {
    if sample_count <= 1 {
        return 1.0;
    }
    sample_index as f64 / (sample_count - 1) as f64
}

fn modulated_frequency(shape_index: usize, x: f64, y: f64, modulation_depth_per_mille: u16) -> f64 {
    let base = 220.0 * (1.0 + shape_index as f64 * 0.15);
    let position_mix = ((x + y) * 0.25 + 0.5).clamp(0.0, 1.0);
    let modulation =
        1.0 + (f64::from(modulation_depth_per_mille) / 1000.0) * (position_mix * 2.0 - 1.0);
    base * modulation
}

fn clamp_i16(sample: f64) -> i16 {
    let clamped = sample.clamp(f64::from(i16::MIN), f64::from(i16::MAX));
    clamped as i16
}

fn lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

fn easing_progress(progress: f64, easing: EasingFamily) -> f64 {
    match easing {
        EasingFamily::Linear => progress,
        EasingFamily::EaseIn => progress * progress,
        EasingFamily::EaseOut => 1.0 - (1.0 - progress) * (1.0 - progress),
        EasingFamily::EaseInOut => {
            if progress < 0.5 {
                2.0 * progress * progress
            } else {
                1.0 - ((-2.0 * progress + 2.0) * (-2.0 * progress + 2.0)) / 2.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EasingFamily;
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

    fn bootstrap_scene() -> SceneGenerationOutput {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 0,
            samples_per_event: 24,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        generate_scene(&params).expect("scene generation should succeed")
    }

    #[test]
    fn render_scene_sound_wav_is_deterministic_for_bootstrap_scene() {
        let config = bootstrap_config();
        let scene = bootstrap_scene();
        let first = render_scene_sound_wav(
            &scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            config.scene.sound_channel_mapping,
        )
        .expect("sound rendering should succeed");
        let second = render_scene_sound_wav(
            &scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            config.scene.sound_channel_mapping,
        )
        .expect("sound rendering should succeed");
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn render_scene_sound_wav_produces_valid_wav() {
        let config = bootstrap_config();
        let scene = bootstrap_scene();
        let wav_bytes = render_scene_sound_wav(
            &scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            config.scene.sound_channel_mapping,
        )
        .expect("sound rendering should succeed");
        let mut reader =
            hound::WavReader::new(Cursor::new(wav_bytes)).expect("generated wav should parse");
        let spec = reader.spec();
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, config.scene.sound_sample_rate_hz);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        let sample_count = reader.samples::<i16>().count();
        assert!(sample_count > 0);
        assert_eq!(sample_count % usize::from(spec.channels), 0);
    }

    #[test]
    fn render_scene_sound_wav_rejects_invalid_shape_index() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_paths: vec![SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![
                    NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                    NormalizedPoint::new(0.1, 0.2).expect("point must build"),
                ],
                soft_memberships: None,
            }],
            motion_events: vec![MotionEvent {
                global_event_index: 0,
                time_slot: 0,
                shape_index: 2,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(-0.2, -0.2).expect("point must build"),
                end_point: NormalizedPoint::new(0.2, 0.2).expect("point must build"),
                duration_frames: 12,
                easing: EasingFamily::EaseInOut,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1],
                generated_per_shape: vec![1],
            },
        };

        let err = render_scene_sound_wav(
            &scene,
            44100,
            24,
            250,
            SoundChannelMapping::StereoAlternating,
        )
        .expect_err("invalid shape index should fail");
        assert!(matches!(
            err,
            SoundEncodingError::ShapeIndexOutOfBounds {
                shape_index: 2,
                shape_count: 1
            }
        ));
    }

    #[test]
    fn render_scene_sound_wav_rejects_zero_duration_events() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_paths: vec![SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![
                    NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                    NormalizedPoint::new(0.1, 0.2).expect("point must build"),
                ],
                soft_memberships: None,
            }],
            motion_events: vec![MotionEvent {
                global_event_index: 7,
                time_slot: 0,
                shape_index: 0,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(-0.2, -0.2).expect("point must build"),
                end_point: NormalizedPoint::new(0.2, 0.2).expect("point must build"),
                duration_frames: 0,
                easing: EasingFamily::EaseInOut,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1],
                generated_per_shape: vec![1],
            },
        };

        let err = render_scene_sound_wav(
            &scene,
            44100,
            24,
            250,
            SoundChannelMapping::StereoAlternating,
        )
        .expect_err("zero-duration event should fail");
        assert!(matches!(
            err,
            SoundEncodingError::ZeroEventDuration {
                global_event_index: 7
            }
        ));
    }
}
