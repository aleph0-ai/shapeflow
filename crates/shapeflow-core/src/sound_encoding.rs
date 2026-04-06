use std::f64::consts::PI;
use std::io::Cursor;

use crate::config::{EasingFamily, SoundChannelMapping};
use crate::scene_generation::SceneGenerationOutput;
use crate::tabular_encoding::{
    canonical_class_count, canonical_class_rank_for_shape_type_and_color,
    shape_type_and_color_indices_for_scene_seed,
};
use libm::{pow, sin};

const PULSE_DUTY_CYCLE: f64 = 0.25;
const OUTPUT_PEAK_CAP_FRACTION: f64 = 0.70;

#[derive(Clone, Copy)]
enum SoundShapeWaveform {
    Sine,
    Triangle,
    Square,
    Sawtooth,
    Pulse,
}

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
    #[error("scene contains no motion events")]
    NoMotionEvents,
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
        return Err(SoundEncodingError::NoMotionEvents);
    }

    let shape_count = scene.shape_paths.len();
    let mut shape_profiles = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        let (shape_type_index, color_index) = match shape_type_and_color_indices_for_scene_seed(
            scene.schedule.scene_layout,
            scene.shape_identity_assignment,
            shape_index,
        ) {
            Ok(indices) => indices,
            Err(error) => {
                return Err(SoundEncodingError::WavEncoding(error.to_string()));
            }
        };
        let canonical_rank =
            canonical_class_rank_for_shape_type_and_color(shape_type_index, color_index)
                .map_err(|error| SoundEncodingError::WavEncoding(error.to_string()))?;
        shape_profiles.push((
            base_frequency_for_canonical_rank(usize::from(canonical_rank)),
            waveform_for_shape_type(shape_type_index),
        ));
    }
    let default_duration_frames = scene
        .motion_events
        .iter()
        .min_by_key(|event| event.time_slot)
        .map(|event| event.duration_frames)
        .ok_or(SoundEncodingError::NoMotionEvents)?;
    let slot_count = usize::try_from(scene.accounting.expected_slots).map_err(|_| {
        SoundEncodingError::WavEncoding("time slot count exceeds platform limits".to_string())
    })?;
    let mut events_by_slot = vec![Vec::new(); slot_count];

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

    let mut interleaved_samples = Vec::<f64>::new();
    let channel_count: u16 = match channel_mapping {
        SoundChannelMapping::MonoMix => 1,
        SoundChannelMapping::StereoAlternating => 2,
    };
    for (slot_index, slot_events) in events_by_slot.iter_mut().enumerate() {
        let expected_duration_frames = if slot_events.is_empty() {
            default_duration_frames
        } else {
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
            expected_duration_frames
        };

        let samples_per_event =
            samples_for_duration(expected_duration_frames, sample_rate_hz, frames_per_second)?;
        let event_gain = (i16::MAX as f64) * 0.35 / (slot_events.len() as f64).max(1.0);
        let modulation_depth = f64::from(modulation_depth_per_mille) / 1000.0;

        for sample_index in 0..samples_per_event {
            let t = slot_progress(sample_index, samples_per_event);
            let mut mono = 0.0_f64;
            let mut left = 0.0_f64;
            let mut right = 0.0_f64;

            for event in slot_events.iter() {
                let eased = easing_progress(t, event.easing);
                let x = lerp(event.start_point.x, event.end_point.x, eased);
                let y = lerp(event.start_point.y, event.end_point.y, eased);
                let (base_frequency, waveform) = shape_profiles[event.shape_index];
                let frequency = modulated_frequency(base_frequency, x, modulation_depth);
                let amplitude = modulated_amplitude(event_gain, y, modulation_depth);
                let sample_time = sample_index as f64 / sample_rate_hz as f64;
                let phase = frequency * sample_time;
                let wave = waveform_sample(waveform, phase) * amplitude;

                match channel_mapping {
                    SoundChannelMapping::MonoMix => {
                        mono += wave;
                    }
                    SoundChannelMapping::StereoAlternating => {
                        let (left_gain, right_gain) = pan_from_x_position(x);
                        left += wave * left_gain;
                        right += wave * right_gain;
                    }
                }
            }

            match channel_mapping {
                SoundChannelMapping::MonoMix => {
                    interleaved_samples.push(mono);
                }
                SoundChannelMapping::StereoAlternating => {
                    interleaved_samples.push(left);
                    interleaved_samples.push(right);
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
    apply_output_peak_cap(&mut interleaved_samples);
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|error| SoundEncodingError::WavEncoding(error.to_string()))?;
        for sample in interleaved_samples {
            writer
                .write_sample(clamp_i16(sample))
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

fn base_frequency_for_canonical_rank(canonical_rank: usize) -> f64 {
    let class_count = canonical_class_count();
    let ratio = canonical_rank as f64 / class_count as f64;
    220.0 * pow(2.0_f64, ratio)
}

fn waveform_for_shape_type(shape_type_index: usize) -> SoundShapeWaveform {
    match shape_type_index {
        0 => SoundShapeWaveform::Sine,
        1 => SoundShapeWaveform::Triangle,
        2 => SoundShapeWaveform::Square,
        3 => SoundShapeWaveform::Sawtooth,
        4 => SoundShapeWaveform::Pulse,
        _ => SoundShapeWaveform::Sine,
    }
}

fn modulated_frequency(base_frequency: f64, x: f64, modulation_depth: f64) -> f64 {
    let clamped_x = x.clamp(-1.0, 1.0);
    base_frequency * (1.0 + modulation_depth * clamped_x)
}

fn modulated_amplitude(event_gain: f64, y: f64, modulation_depth: f64) -> f64 {
    let clamped_y = y.clamp(-1.0, 1.0);
    event_gain * (1.0 + modulation_depth * clamped_y)
}

fn pan_from_x_position(x: f64) -> (f64, f64) {
    let clamped_x = x.clamp(-1.0, 1.0);
    let pan = (clamped_x + 1.0) * 0.5;
    (1.0 - pan, pan)
}

fn waveform_sample(shape_waveform: SoundShapeWaveform, phase: f64) -> f64 {
    let periodic_phase = phase - phase.floor();
    match shape_waveform {
        SoundShapeWaveform::Sine => sin(2.0 * PI * periodic_phase),
        SoundShapeWaveform::Triangle => 2.0 * (2.0 * periodic_phase - 1.0).abs() - 1.0,
        SoundShapeWaveform::Square => {
            if periodic_phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        SoundShapeWaveform::Sawtooth => 2.0 * periodic_phase - 1.0,
        SoundShapeWaveform::Pulse => {
            if periodic_phase < PULSE_DUTY_CYCLE {
                1.0
            } else {
                -1.0
            }
        }
    }
}

fn clamp_i16(sample: f64) -> i16 {
    let clamped = sample.clamp(f64::from(i16::MIN), f64::from(i16::MAX));
    clamped as i16
}

fn peak_cap_value() -> f64 {
    f64::from(i16::MAX) * OUTPUT_PEAK_CAP_FRACTION
}

fn apply_output_peak_cap(samples: &mut [f64]) {
    if samples.is_empty() {
        return;
    }

    let cap = peak_cap_value();
    if cap <= 0.0 {
        return;
    }

    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f64, f64::max);
    if peak <= cap {
        return;
    }

    let gain = cap / peak;
    for sample in samples.iter_mut() {
        *sample *= gain;
    }
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

    fn synthesize_single_event_scene(
        shape_count: usize,
        shape_index: usize,
        point: NormalizedPoint,
    ) -> SceneGenerationOutput {
        let mut generated_per_shape = vec![0_u16; shape_count];
        generated_per_shape[shape_index] = generated_per_shape[shape_index]
            .checked_add(1)
            .expect("per-shape count should fit u16");

        SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(7, 0),
            shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
            shape_paths: (0..shape_count)
                .map(|idx| SceneShapePath {
                    shape_index: idx,
                    trajectory_points: vec![point, point],
                    soft_memberships: None,
                })
                .collect(),
            motion_events: vec![MotionEvent {
                global_event_index: 0,
                time_slot: 0,
                shape_index,
                shape_event_index: 0,
                start_point: point,
                end_point: point,
                duration_frames: 24,
                easing: EasingFamily::Linear,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                expected_slots: 1,
                generated_total: 1,
                expected_per_shape: generated_per_shape.clone(),
                generated_per_shape,
            },
        }
    }

    fn decode_i16_samples(wav_bytes: &[u8]) -> Vec<i16> {
        let mut reader =
            hound::WavReader::new(Cursor::new(wav_bytes)).expect("generated wav should parse");
        reader
            .samples::<i16>()
            .map(|sample| sample.expect("sample should decode"))
            .collect()
    }

    fn split_stereo_samples(wav_bytes: &[u8]) -> (Vec<i16>, Vec<i16>) {
        let interleaved = decode_i16_samples(wav_bytes);
        (
            interleaved.iter().step_by(2).copied().collect(),
            interleaved.iter().skip(1).step_by(2).copied().collect(),
        )
    }

    fn mean_abs_sample(samples: &[i16]) -> f64 {
        let sum = samples
            .iter()
            .map(|sample| f64::from(*sample).abs())
            .sum::<f64>();
        sum / samples.len() as f64
    }

    fn peak_abs_float_sample(samples: &[f64]) -> f64 {
        samples
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f64, f64::max)
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
    fn render_scene_sound_wav_output_peak_is_capped() {
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
        let samples = decode_i16_samples(&wav_bytes);
        let peak = samples
            .iter()
            .map(|sample| f64::from(*sample).abs())
            .fold(0.0_f64, f64::max);
        assert!(peak <= peak_cap_value() + 1.0);
    }

    #[test]
    fn render_scene_sound_wav_rejects_invalid_shape_index() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
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
                expected_slots: 1,
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
            shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
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
                expected_slots: 1,
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

    #[test]
    fn render_scene_sound_wav_rejects_empty_scene() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
            shape_paths: Vec::new(),
            motion_events: Vec::new(),
            accounting: MotionEventAccounting {
                expected_total: 0,
                expected_slots: 0,
                generated_total: 0,
                expected_per_shape: Vec::new(),
                generated_per_shape: Vec::new(),
            },
        };

        let err = render_scene_sound_wav(
            &scene,
            44100,
            24,
            250,
            SoundChannelMapping::StereoAlternating,
        )
        .expect_err("empty scene should fail");
        assert!(matches!(err, SoundEncodingError::NoMotionEvents));
    }

    #[test]
    fn render_scene_sound_wav_shape_color_affects_base_frequency() {
        let config = bootstrap_config();
        let red_scene = synthesize_single_event_scene(
            2,
            0,
            NormalizedPoint::new(0.0, 0.0).expect("point must build"),
        );
        let blue_scene = synthesize_single_event_scene(
            2,
            1,
            NormalizedPoint::new(0.0, 0.0).expect("point must build"),
        );

        let red_bytes = render_scene_sound_wav(
            &red_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::MonoMix,
        )
        .expect("rendering should succeed");
        let blue_bytes = render_scene_sound_wav(
            &blue_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::MonoMix,
        )
        .expect("rendering should succeed");

        let red_samples = decode_i16_samples(&red_bytes);
        let blue_samples = decode_i16_samples(&blue_bytes);
        assert!(
            red_samples
                .iter()
                .zip(blue_samples.iter())
                .any(|(left, right)| left != right),
            "different colors should drive different base frequencies"
        );
    }

    #[test]
    fn render_scene_sound_wav_shape_type_affects_waveform() {
        let config = bootstrap_config();
        let circle_scene = synthesize_single_event_scene(
            6,
            0,
            NormalizedPoint::new(0.0, 0.0).expect("point must build"),
        );
        let pentagon_scene = synthesize_single_event_scene(
            6,
            4,
            NormalizedPoint::new(0.0, 0.0).expect("point must build"),
        );

        let circle_bytes = render_scene_sound_wav(
            &circle_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::MonoMix,
        )
        .expect("rendering should succeed");
        let pentagon_bytes = render_scene_sound_wav(
            &pentagon_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::MonoMix,
        )
        .expect("rendering should succeed");

        let circle_samples = decode_i16_samples(&circle_bytes);
        let pentagon_samples = decode_i16_samples(&pentagon_bytes);
        assert!(
            circle_samples
                .iter()
                .zip(pentagon_samples.iter())
                .any(|(left, right)| left != right),
            "different shape types should affect waveform signature"
        );
    }

    #[test]
    fn render_scene_sound_wav_y_position_affects_amplitude() {
        let config = bootstrap_config();
        let low_scene = synthesize_single_event_scene(
            1,
            0,
            NormalizedPoint::new(0.0, -1.0).expect("point must build"),
        );
        let high_scene = synthesize_single_event_scene(
            1,
            0,
            NormalizedPoint::new(0.0, 1.0).expect("point must build"),
        );

        let low_bytes = render_scene_sound_wav(
            &low_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::MonoMix,
        )
        .expect("rendering should succeed");
        let high_bytes = render_scene_sound_wav(
            &high_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::MonoMix,
        )
        .expect("rendering should succeed");

        let low_samples = decode_i16_samples(&low_bytes);
        let high_samples = decode_i16_samples(&high_bytes);
        assert!(
            mean_abs_sample(&high_samples) > mean_abs_sample(&low_samples) * 1.5,
            "high y should increase sample amplitude"
        );
    }

    #[test]
    fn apply_output_peak_cap_does_not_boost_quiet_samples() {
        let mut quiet_samples = vec![120.0_f64, -300.0, 80.0, -40.0, 300.0];
        let original = quiet_samples.clone();
        apply_output_peak_cap(&mut quiet_samples);
        assert_eq!(quiet_samples, original);
    }

    #[test]
    fn apply_output_peak_cap_reduces_loud_samples_to_cap() {
        let mut loud_samples = vec![
            f64::from(i16::MAX),
            f64::from(i16::MIN + 1),
            20_000.0,
            -18_000.0,
        ];
        apply_output_peak_cap(&mut loud_samples);
        let peak = peak_abs_float_sample(&loud_samples);
        assert!(peak <= peak_cap_value() + 1.0e-9);
        assert!(peak > 0.0);
    }

    #[test]
    fn render_scene_sound_wav_stereo_panning_tracks_x_position() {
        let config = bootstrap_config();
        let left_scene = synthesize_single_event_scene(
            1,
            0,
            NormalizedPoint::new(-1.0, 0.0).expect("point must build"),
        );
        let right_scene = synthesize_single_event_scene(
            1,
            0,
            NormalizedPoint::new(1.0, 0.0).expect("point must build"),
        );
        let center_scene = synthesize_single_event_scene(
            1,
            0,
            NormalizedPoint::new(0.0, 0.0).expect("point must build"),
        );

        let left_bytes = render_scene_sound_wav(
            &left_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::StereoAlternating,
        )
        .expect("rendering should succeed");
        let right_bytes = render_scene_sound_wav(
            &right_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::StereoAlternating,
        )
        .expect("rendering should succeed");
        let center_bytes = render_scene_sound_wav(
            &center_scene,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            SoundChannelMapping::StereoAlternating,
        )
        .expect("rendering should succeed");

        let (left_only_l, left_only_r) = split_stereo_samples(&left_bytes);
        let (right_only_l, right_only_r) = split_stereo_samples(&right_bytes);
        let (center_l, center_r) = split_stereo_samples(&center_bytes);

        assert!(left_only_r.iter().all(|sample| *sample == 0));
        assert!(right_only_l.iter().all(|sample| *sample == 0));
        assert!(center_l.iter().any(|sample| *sample != 0));
        assert!(center_r.iter().any(|sample| *sample != 0));
        assert!(!left_only_l.iter().all(|sample| *sample == 0));
        assert!(!right_only_r.iter().all(|sample| *sample == 0));
    }
}
