use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gif::{Encoder, Frame, Repeat};
use hound;
use shapeflow_core::{
    EasingFamily, MotionEvent, MotionEventAccounting, NormalizedPoint, SceneGenerationOutput,
    ShapeFlowConfig,
    image_encoding::render_scene_image_png_with_scene_config,
    sound_encoding::render_scene_sound_wav,
    tabular_encoding::{
        generate_tabular_motion_rows, serialize_tabular_motion_rows_csv_display, shape_identity_for_scene,
    },
    text_encoding::{
        generate_scene_text_lines_with_scene_config_and_profile, serialize_scene_text,
    },
    video_encoding::render_scene_video_frames_png_with_keyframe_border,
};

use crate::flow::{self, Difficulty, Modality, PlanItem};
use std::io::Cursor;

/// Remove the first line; optionally keep or drop Pair lines for display.
fn trim_text_for_display(text: &str, include_pairs: bool) -> String {
    let mut lines = text.lines();
    lines.next(); // skip first line
    let mut out = String::with_capacity(text.len());
    for line in lines {
        if !include_pairs && line.starts_with("Pair ") {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.truncate(out.trim_end().len());
    out
}

const MAX_VIDEO_PREVIEW_RESOLUTION: u32 = 256;
const MAX_VIDEO_PLAYER_FRAMES: usize = 180;
const MAX_VIDEO_GIF_FRAMES: usize = 120;
const PREVIEW_AUDIO_PEAK_TARGET_FRACTION: f64 = 0.70;

#[derive(Debug, Clone)]
pub enum TaskStimulus {
    Image {
        data_uri: String,
    },
    VideoPlayer {
        frame_data_uris: Vec<String>,
        fps: u16,
    },
    VideoGif {
        data_uri: String,
    },
    Text {
        body: String,
    },
    TabularCsv {
        csv: String,
    },
    Sound {
        data_uri: String,
        shape_previews: Vec<SoundShapePreview>,
        quadrant_guide_data_uri: String,
        transition_previews: Vec<SoundTransitionPreview>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeSamplePayload {
    Text { mime_type: String, text: String },
    Binary { mime_type: String, bytes: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct SoundShapePreview {
    pub label: String,
    pub image_data_uri: String,
    pub tone_data_uri: String,
}

#[derive(Debug, Clone)]
pub struct SoundTransitionPreview {
    pub label: String,
    pub audio_data_uri: String,
}

pub fn build_task_stimulus(
    seed: u64,
    difficulty: Difficulty,
    item: &PlanItem,
    is_human: bool,
) -> Result<TaskStimulus> {
    let (config, scene) = build_config_and_scene(seed, difficulty, item.scene_index)
        .context("failed to build task stimulus inputs")?;

    match item.modality {
        Modality::Image => {
            let png = render_scene_image_png_with_scene_config(&scene, &config.scene)
                .map_err(|error| anyhow::anyhow!("{error}"))
                .context("failed to render image stimulus")?;
            Ok(TaskStimulus::Image {
                data_uri: png_data_uri(&png),
            })
        }
        Modality::Video => {
            let preview_resolution = config
                .scene
                .resolution
                .min(MAX_VIDEO_PREVIEW_RESOLUTION)
                .max(1);
            let frames = render_scene_video_frames_png_with_keyframe_border(
                &scene,
                preview_resolution,
                config.scene.video_keyframe_border,
            )
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("failed to render video frames stimulus")?;

            if frames.is_empty() {
                return Ok(TaskStimulus::VideoPlayer {
                    frame_data_uris: Vec::new(),
                    fps: config.scene.sound_frames_per_second,
                });
            }

            if is_human {
                let frame_data_uris = downsample_indices(frames.len(), MAX_VIDEO_PLAYER_FRAMES)
                    .into_iter()
                    .map(|index| png_data_uri(&frames[index]))
                    .collect();
                Ok(TaskStimulus::VideoPlayer {
                    frame_data_uris,
                    fps: config.scene.sound_frames_per_second,
                })
            } else {
                let gif = encode_gif_from_png_frames(
                    &frames,
                    MAX_VIDEO_GIF_FRAMES,
                    config.scene.sound_frames_per_second,
                )
                .context("failed to encode GIF video stimulus")?;
                Ok(TaskStimulus::VideoGif {
                    data_uri: gif_data_uri(&gif),
                })
            }
        }
        Modality::Text => {
            let profile = flow::text_alteration_profile_for_difficulty(difficulty);
            let lines = generate_scene_text_lines_with_scene_config_and_profile(
                &scene,
                &config.scene,
                profile,
            )
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("failed to render text stimulus")?;
            let full_text = serialize_scene_text(&lines);
            let trimmed = trim_text_for_display(&full_text, is_human);
            Ok(TaskStimulus::Text { body: trimmed })
        }
        Modality::Tabular => {
            let rows = generate_tabular_motion_rows(&scene)
                .map_err(|error| anyhow::anyhow!("{error}"))
                .context("failed to render tabular stimulus rows")?;
            Ok(TaskStimulus::TabularCsv {
                csv: serialize_tabular_motion_rows_csv_display(&rows),
            })
        }
        Modality::Sound => {
            let wav = render_scene_sound_wav(
                &scene,
                config.scene.sound_sample_rate_hz,
                config.scene.sound_frames_per_second,
                config.scene.sound_modulation_depth_per_mille,
                config.scene.sound_channel_mapping,
            )
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("failed to render sound stimulus")?;
            let wav = normalize_preview_wav_peak(&wav)
                .context("failed to normalize sound WAV before encoding")?;
            let (shape_previews, quadrant_guide_data_uri, transition_previews) =
                build_sound_guidance(&scene, &config, item)
                    .context("failed to render sound guidance stimuli")?;
            Ok(TaskStimulus::Sound {
                data_uri: wav_data_uri(&wav),
                shape_previews,
                quadrant_guide_data_uri,
                transition_previews,
            })
        }
    }
}

pub fn build_ai_native_sample(
    seed: u64,
    difficulty: Difficulty,
    modality: Modality,
    scene_index: u32,
) -> Result<NativeSamplePayload> {
    let (config, scene) = build_config_and_scene(seed, difficulty, scene_index)
        .context("failed to build native sample inputs")?;

    match modality {
        Modality::Image => {
            let png = render_scene_image_png_with_scene_config(&scene, &config.scene)
                .map_err(|error| anyhow::anyhow!("{error}"))
                .context("failed to render native image sample")?;
            Ok(NativeSamplePayload::Binary {
                mime_type: "image/png".to_string(),
                bytes: png,
            })
        }
        Modality::Video => {
            let preview_resolution = config
                .scene
                .resolution
                .min(MAX_VIDEO_PREVIEW_RESOLUTION)
                .max(1);
            let frames = render_scene_video_frames_png_with_keyframe_border(
                &scene,
                preview_resolution,
                config.scene.video_keyframe_border,
            )
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("failed to render native video sample frames")?;
            let gif = encode_gif_from_png_frames(
                &frames,
                MAX_VIDEO_GIF_FRAMES,
                config.scene.sound_frames_per_second,
            )
            .context("failed to encode native video sample GIF")?;
            Ok(NativeSamplePayload::Binary {
                mime_type: "image/gif".to_string(),
                bytes: gif,
            })
        }
        Modality::Text => {
            let profile = flow::text_alteration_profile_for_difficulty(difficulty);
            let lines = generate_scene_text_lines_with_scene_config_and_profile(
                &scene,
                &config.scene,
                profile,
            )
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("failed to render native text sample")?;
            Ok(NativeSamplePayload::Text {
                mime_type: "text/plain; charset=utf-8".to_string(),
                text: serialize_scene_text(&lines),
            })
        }
        Modality::Tabular => {
            let rows = generate_tabular_motion_rows(&scene)
                .map_err(|error| anyhow::anyhow!("{error}"))
                .context("failed to render native tabular sample rows")?;
            Ok(NativeSamplePayload::Text {
                mime_type: "text/csv; charset=utf-8".to_string(),
                text: serialize_tabular_motion_rows_csv_display(&rows),
            })
        }
        Modality::Sound => {
            let wav = render_scene_sound_wav(
                &scene,
                config.scene.sound_sample_rate_hz,
                config.scene.sound_frames_per_second,
                config.scene.sound_modulation_depth_per_mille,
                config.scene.sound_channel_mapping,
            )
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("failed to render native sound sample")?;
            Ok(NativeSamplePayload::Binary {
                mime_type: "audio/wav".to_string(),
                bytes: wav,
            })
        }
    }
}

pub fn build_ai_native_sound_reference(
    seed: u64,
    difficulty: Difficulty,
    scene_index: u32,
    shape_id: &str,
) -> Result<Vec<u8>> {
    let (config, scene) = build_config_and_scene(seed, difficulty, scene_index)
        .context("failed to build sound reference inputs")?;

    let shape_index = (0..scene.shape_paths.len())
        .find(|&shape_index| {
            shape_identity_for_scene(&scene, shape_index)
                .ok()
                .is_some_and(|identity| identity.shape_id == shape_id)
        })
        .ok_or_else(|| anyhow::anyhow!("shape id '{shape_id}' is not present in this scene"))?;

    let wav = render_preview_motion_wav(
        &scene,
        &config,
        shape_index,
        (0.0, 0.0),
        (0.0, 0.0),
        EasingFamily::Linear,
    )
    .context("failed to render sound reference preview")?;
    normalize_preview_wav_peak(&wav).context("failed to normalize sound reference WAV")
}

fn build_config_and_scene(
    seed: u64,
    difficulty: Difficulty,
    scene_index: u32,
) -> Result<(ShapeFlowConfig, SceneGenerationOutput)> {
    let config = flow::build_session_config(seed, difficulty)
        .map_err(|error| anyhow::anyhow!("{error}"))
        .context("failed to build session config")?;
    let scene = flow::build_scene_for_seed(seed, difficulty, scene_index)
        .map_err(|error| anyhow::anyhow!("{error}"))
        .context("failed to generate scene")?;
    Ok((config, scene))
}

fn build_sound_guidance(
    scene: &SceneGenerationOutput,
    config: &ShapeFlowConfig,
    item: &PlanItem,
) -> Result<(Vec<SoundShapePreview>, String, Vec<SoundTransitionPreview>)> {
    let shape_count = scene.shape_paths.len();
    if shape_count == 0 {
        return Err(anyhow::anyhow!(
            "cannot build sound guidance for zero-shape scene"
        ));
    }

    let mut shape_previews = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        let identity = shape_identity_for_scene(scene, shape_index)
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("failed to resolve shape identity for sound guidance")?;
        let tone = render_preview_motion_wav(
            scene,
            config,
            shape_index,
            (0.0, 0.0),
            (0.0, 0.0),
            EasingFamily::Linear,
        )
        .context("failed to render per-shape tone preview")?;
        let tone = normalize_preview_wav_peak(&tone)
            .context("failed to normalize per-shape tone preview")?;
        shape_previews.push(SoundShapePreview {
            label: flow::shape_id_to_natural_label(&identity.shape_id),
            image_data_uri: shape_preview_svg_data_uri(&identity.shape_type, &identity.color),
            tone_data_uri: wav_data_uri(&tone),
        });
    }

    let reference_shape_index = match item.query_shape.as_ref() {
        Some(query_shape_id) => (0..shape_count)
            .find(|shape_index| {
                shape_identity_for_scene(scene, *shape_index)
                    .map(|identity| identity.shape_id == *query_shape_id)
                    .unwrap_or(false)
            })
            .unwrap_or(0),
        None => 0,
    };

    let transitions = [
        ("4 -> 1", (0.5, -0.5), (0.5, 0.5)),
        ("1 -> 2", (0.5, 0.5), (-0.5, 0.5)),
        ("2 -> 3", (-0.5, 0.5), (-0.5, -0.5)),
        ("3 -> 4", (-0.5, -0.5), (0.5, -0.5)),
    ];
    let mut transition_previews = Vec::with_capacity(transitions.len());
    for (label, start, end) in transitions {
        let audio = render_preview_motion_wav(
            scene,
            config,
            reference_shape_index,
            start,
            end,
            EasingFamily::Linear,
        )
        .context("failed to render quadrant transition example audio")?;
        let audio = normalize_preview_wav_peak(&audio)
            .context("failed to normalize transition preview audio")?;
        transition_previews.push(SoundTransitionPreview {
            label: label.to_string(),
            audio_data_uri: wav_data_uri(&audio),
        });
    }

    Ok((
        shape_previews,
        quadrant_guide_svg_data_uri(),
        transition_previews,
    ))
}

fn render_preview_motion_wav(
    scene: &SceneGenerationOutput,
    config: &ShapeFlowConfig,
    shape_index: usize,
    start_xy: (f64, f64),
    end_xy: (f64, f64),
    easing: EasingFamily,
) -> Result<Vec<u8>> {
    if shape_index >= scene.shape_paths.len() {
        return Err(anyhow::anyhow!(
            "shape index {} out of range for preview scene with {} shapes",
            shape_index,
            scene.shape_paths.len()
        ));
    }

    let mut per_shape = vec![0_u16; scene.shape_paths.len()];
    per_shape[shape_index] = 1;
    let duration_frames = scene
        .motion_events
        .first()
        .map(|event| event.duration_frames.max(1))
        .unwrap_or(24);
    let preview_scene = SceneGenerationOutput {
        scene_index: scene.scene_index,
        schedule: scene.schedule,
        shape_identity_assignment: scene.shape_identity_assignment,
        shape_paths: scene.shape_paths.clone(),
        motion_events: vec![MotionEvent {
            global_event_index: 0,
            time_slot: 0,
            shape_index,
            shape_event_index: 0,
            start_point: NormalizedPoint::new(start_xy.0, start_xy.1)
                .map_err(|error| anyhow::anyhow!("{error}"))?,
            end_point: NormalizedPoint::new(end_xy.0, end_xy.1)
                .map_err(|error| anyhow::anyhow!("{error}"))?,
            duration_frames,
            easing,
        }],
        accounting: MotionEventAccounting {
            expected_total: 1,
            expected_slots: 1,
            generated_total: 1,
            expected_per_shape: per_shape.clone(),
            generated_per_shape: per_shape,
        },
    };

    render_scene_sound_wav(
        &preview_scene,
        config.scene.sound_sample_rate_hz,
        config.scene.sound_frames_per_second,
        config.scene.sound_modulation_depth_per_mille,
        config.scene.sound_channel_mapping,
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
    .context("preview WAV render failed")
}

fn normalize_preview_wav_peak(wav_bytes: &[u8]) -> Result<Vec<u8>> {
    let mut reader = hound::WavReader::new(Cursor::new(wav_bytes))
        .context("failed to decode WAV preview clip")?;
    let spec = reader.spec();

    if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
        return Err(anyhow::anyhow!(
            "unsupported WAV format for preview normalization (expects int16): {:?}, {} bits",
            spec.sample_format,
            spec.bits_per_sample
        ));
    }

    let mut samples = Vec::with_capacity(reader.len() as usize);
    let mut peak = 0.0_f64;
    for sample in reader.samples::<i16>() {
        let sample = sample.context("failed to decode WAV sample for preview normalization")?;
        peak = peak.max(f64::from(sample).abs());
        samples.push(sample);
    }

    if peak == 0.0 {
        return Ok(wav_bytes.to_vec());
    }

    let target_peak = PREVIEW_AUDIO_PEAK_TARGET_FRACTION * f64::from(i16::MAX);
    let gain = target_peak / peak;

    let mut normalized = Vec::with_capacity(samples.len());
    for sample in samples {
        let scaled = (f64::from(sample) * gain).round();
        let clamped = scaled.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
        normalized.push(clamped);
    }

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut cursor, spec).context("failed to create WAV writer")?;
        for sample in normalized {
            writer
                .write_sample(sample)
                .context("failed to write normalized WAV sample")?;
        }
        writer
            .finalize()
            .context("failed to finalize normalized WAV preview")?;
    }

    Ok(cursor.into_inner())
}

fn shape_preview_svg_data_uri(shape_type: &str, color: &str) -> String {
    let fill = color_name_to_hex(color);
    let shape_markup = match shape_type {
        "circle" => String::from(r#"<circle cx="60" cy="60" r="30" />"#),
        "square" => String::from(r#"<rect x="30" y="30" width="60" height="60" rx="4" ry="4" />"#),
        "triangle" => String::from(r#"<polygon points="60,26 94,88 26,88" />"#),
        "pentagon" => format!(
            r#"<polygon points="{}" />"#,
            regular_polygon_points(5, 60.0, 60.0, 35.0, -std::f64::consts::FRAC_PI_2)
        ),
        "hexagon" => format!(
            r#"<polygon points="{}" />"#,
            regular_polygon_points(6, 60.0, 60.0, 35.0, -std::f64::consts::FRAC_PI_2)
        ),
        "star" => format!(
            r#"<polygon points="{}" />"#,
            star_points(60.0, 60.0, 35.0, 16.0, -std::f64::consts::FRAC_PI_2)
        ),
        _ => String::from(r#"<circle cx="60" cy="60" r="30" />"#),
    };
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120" width="120" height="120">
<rect x="1.5" y="1.5" width="117" height="117" rx="10" ry="10" fill="#f7fbff" stroke="#bfd0e8" stroke-width="2"/>
<g fill="{fill}" stroke="#1b2532" stroke-width="4">{shape_markup}</g>
</svg>"##
    );
    svg_data_uri(&svg)
}

fn quadrant_guide_svg_data_uri() -> String {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 240" width="240" height="240">
<rect x="40" y="40" width="160" height="160" rx="8" ry="8" fill="#f7fbff" stroke="#1f2a37" stroke-width="3"/>
<line x1="120" y1="40" x2="120" y2="200" stroke="#1f2a37" stroke-width="2"/>
<line x1="40" y1="120" x2="200" y2="120" stroke="#1f2a37" stroke-width="2"/>
<text x="155" y="86" font-size="22" fill="#1f2a37" text-anchor="middle">1</text>
<text x="85" y="86" font-size="22" fill="#1f2a37" text-anchor="middle">2</text>
<text x="85" y="166" font-size="22" fill="#1f2a37" text-anchor="middle">3</text>
<text x="155" y="166" font-size="22" fill="#1f2a37" text-anchor="middle">4</text>

<line x1="214" y1="170" x2="214" y2="70" stroke="#2b6cb0" stroke-width="4" stroke-linecap="round"/>
<polygon points="214,58 208,72 220,72" fill="#2b6cb0"/>
<text x="224" y="118" font-size="14" fill="#2b6cb0" text-anchor="start">4→1</text>

<line x1="170" y1="26" x2="70" y2="26" stroke="#2b6cb0" stroke-width="4" stroke-linecap="round"/>
<polygon points="58,26 72,20 72,32" fill="#2b6cb0"/>
<text x="120" y="18" font-size="14" fill="#2b6cb0" text-anchor="middle">1→2</text>

<line x1="26" y1="70" x2="26" y2="170" stroke="#2b6cb0" stroke-width="4" stroke-linecap="round"/>
<polygon points="26,182 20,168 32,168" fill="#2b6cb0"/>
<text x="16" y="122" font-size="14" fill="#2b6cb0" text-anchor="end">2→3</text>

<line x1="70" y1="214" x2="170" y2="214" stroke="#2b6cb0" stroke-width="4" stroke-linecap="round"/>
<polygon points="182,214 168,208 168,220" fill="#2b6cb0"/>
<text x="120" y="234" font-size="14" fill="#2b6cb0" text-anchor="middle">3→4</text>
</svg>"##;
    svg_data_uri(svg)
}

fn regular_polygon_points(sides: usize, cx: f64, cy: f64, radius: f64, start_angle: f64) -> String {
    let mut points = Vec::with_capacity(sides);
    for index in 0..sides {
        let angle = start_angle + (index as f64) * (2.0 * std::f64::consts::PI / sides as f64);
        points.push(format!(
            "{:.2},{:.2}",
            cx + radius * angle.cos(),
            cy + radius * angle.sin()
        ));
    }
    points.join(" ")
}

fn star_points(cx: f64, cy: f64, outer_radius: f64, inner_radius: f64, start_angle: f64) -> String {
    let mut points = Vec::with_capacity(10);
    for index in 0..10 {
        let radius = if index % 2 == 0 {
            outer_radius
        } else {
            inner_radius
        };
        let angle = start_angle + (index as f64) * (std::f64::consts::PI / 5.0);
        points.push(format!(
            "{:.2},{:.2}",
            cx + radius * angle.cos(),
            cy + radius * angle.sin()
        ));
    }
    points.join(" ")
}

fn color_name_to_hex(color: &str) -> &'static str {
    match color {
        "red" => "#e53e3e",
        "green" => "#2f855a",
        "blue" => "#2b6cb0",
        "yellow" => "#d69e2e",
        "magenta" => "#b83280",
        "cyan" => "#0d9488",
        _ => "#718096",
    }
}

fn downsample_indices(frame_count: usize, max_frames: usize) -> Vec<usize> {
    if frame_count == 0 {
        return Vec::new();
    }
    if max_frames == 0 || frame_count <= max_frames {
        return (0..frame_count).collect();
    }

    let stride = (frame_count + max_frames - 1) / max_frames;
    let mut indices = (0..frame_count).step_by(stride).collect::<Vec<_>>();
    let last = frame_count - 1;
    if indices.last().copied() != Some(last) {
        indices.push(last);
    }
    indices
}

fn encode_gif_from_png_frames(
    png_frames: &[Vec<u8>],
    max_frames: usize,
    fps: u16,
) -> Result<Vec<u8>> {
    let indices = downsample_indices(png_frames.len(), max_frames);
    if indices.is_empty() {
        return Ok(Vec::new());
    }

    let first = image::load_from_memory(&png_frames[indices[0]])
        .context("failed to decode first PNG frame for GIF")?
        .to_rgba8();
    let width = first.width();
    let height = first.height();
    let width_u16 = u16::try_from(width).context("gif frame width exceeds u16")?;
    let height_u16 = u16::try_from(height).context("gif frame height exceeds u16")?;
    let delay_cs = frame_delay_centiseconds(fps);

    let mut gif_bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut gif_bytes, width_u16, height_u16, &[])
            .context("failed to create GIF encoder")?;
        encoder
            .set_repeat(Repeat::Infinite)
            .context("failed to set GIF repeat mode")?;

        for frame_index in indices {
            let decoded = image::load_from_memory(&png_frames[frame_index])
                .context("failed to decode PNG frame for GIF")?
                .to_rgba8();
            if decoded.width() != width || decoded.height() != height {
                return Err(anyhow::anyhow!(
                    "video frame dimensions changed between frames"
                ));
            }
            let mut rgba = decoded.into_raw();
            let mut frame = Frame::from_rgba_speed(width_u16, height_u16, &mut rgba, 10);
            frame.delay = delay_cs;
            encoder
                .write_frame(&frame)
                .context("failed to write GIF frame")?;
        }
    }

    Ok(gif_bytes)
}

fn frame_delay_centiseconds(fps: u16) -> u16 {
    let fps = u32::from(fps.max(1));
    let delay = (100 + fps / 2) / fps;
    delay.max(1) as u16
}

fn png_data_uri(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
}

fn gif_data_uri(bytes: &[u8]) -> String {
    format!("data:image/gif;base64,{}", STANDARD.encode(bytes))
}

fn svg_data_uri(svg: &str) -> String {
    format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg))
}

fn wav_data_uri(bytes: &[u8]) -> String {
    format!("data:audio/wav;base64,{}", STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_i16_wav(samples: &[i16], channels: u16, sample_rate: u32) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer =
                hound::WavWriter::new(&mut cursor, spec).expect("failed to create test WAV writer");
            for sample in samples {
                writer
                    .write_sample(*sample)
                    .expect("failed to write test WAV sample");
            }
            writer.finalize().expect("failed to finalize test WAV");
        }
        cursor.into_inner()
    }

    fn wav_peak_i16(wav_bytes: &[u8]) -> i32 {
        let mut reader =
            hound::WavReader::new(Cursor::new(wav_bytes)).expect("failed to parse WAV");
        reader
            .samples::<i16>()
            .map(|sample| sample.expect("failed to read WAV sample"))
            .map(i32::from)
            .map(i32::abs)
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn normalize_preview_wav_peak_preserves_format_and_length() {
        let input = encode_i16_wav(&[0, 1000, -2000, 3000, -4000, 5000], 2, 44_100);
        let output = normalize_preview_wav_peak(&input).expect("normalization failed");

        let input_reader =
            hound::WavReader::new(Cursor::new(&input)).expect("failed to decode input WAV");
        let output_reader =
            hound::WavReader::new(Cursor::new(&output)).expect("failed to decode output WAV");

        assert_eq!(input_reader.spec().channels, output_reader.spec().channels);
        assert_eq!(
            input_reader.spec().sample_rate,
            output_reader.spec().sample_rate
        );
        assert_eq!(
            input_reader.spec().bits_per_sample,
            output_reader.spec().bits_per_sample
        );
        assert_eq!(
            input_reader.spec().sample_format,
            output_reader.spec().sample_format
        );
        assert_eq!(input_reader.len(), output_reader.len());
    }

    #[test]
    fn normalize_preview_wav_peak_raises_quiet_clip_level_to_target() {
        let sample_count = 16_384_usize;
        let sample_rate = 44_100;
        let quiet_input = encode_i16_wav(&vec![250_i16; sample_count], 1, sample_rate);
        let loud_input = encode_i16_wav(&vec![12_000_i16; sample_count], 1, sample_rate);

        let quiet_before = wav_peak_i16(&quiet_input) as f64;
        let loud_before = wav_peak_i16(&loud_input) as f64;
        let quiet_after =
            wav_peak_i16(&normalize_preview_wav_peak(&quiet_input).expect("normalize")) as f64;
        let loud_after =
            wav_peak_i16(&normalize_preview_wav_peak(&loud_input).expect("normalize")) as f64;

        assert!(loud_before / quiet_before > 20.0);
        assert!(quiet_after >= 0.68 * f64::from(i16::MAX));
        assert!(loud_after >= 0.68 * f64::from(i16::MAX));
        assert!(loud_after / quiet_after < 1.05);
    }

    #[test]
    fn trim_text_for_display_keeps_pair_lines_for_human_mode() {
        let input = "\
Scene 00000000 events=2
Event 0000: alpha
Event 0001: beta
Pair 00-01: relation
Pair 01-00: relation
";

        let human = trim_text_for_display(input, true);
        let ai = trim_text_for_display(input, false);

        assert!(human.contains("Pair 00-01: relation"));
        assert!(human.contains("Pair 01-00: relation"));
        assert!(!human.contains("Scene 00000000 events=2"));

        assert!(!ai.contains("Pair 00-01: relation"));
        assert!(!ai.contains("Pair 01-00: relation"));
        assert!(ai.contains("Event 0000: alpha"));
        assert!(!ai.contains("Scene 00000000 events=2"));
    }

    #[test]
    fn build_config_and_scene_uses_seed_mapped_scene_index() {
        let seed = (1u64 << 17) + 42;
        let difficulty = Difficulty::Hard;
        let scene_index = 7u32;

        let (_config, mapped_scene) =
            build_config_and_scene(seed, difficulty, scene_index).expect("scene should build");
        let expected_scene =
            flow::build_scene_for_seed(seed, difficulty, scene_index).expect("scene should build");
        let raw_config =
            flow::build_session_config(seed, difficulty).expect("session config should build");
        let raw_scene =
            flow::build_scene_for_index(&raw_config, scene_index).expect("raw scene should build");

        assert_eq!(mapped_scene, expected_scene);
        assert_ne!(mapped_scene.scene_index, raw_scene.scene_index);
    }

    #[test]
    fn build_ai_native_sample_tabular_uses_display_schema() {
        let payload = build_ai_native_sample(42, Difficulty::Medium, Modality::Tabular, 3)
            .expect("tabular sample should build");

        let NativeSamplePayload::Text { text, .. } = payload else {
            panic!("tabular native sample should be text payload");
        };

        let header = text
            .lines()
            .next()
            .expect("tabular CSV should contain a header row");

        assert!(!header.contains("scene_id"));
        assert!(header.contains("event_index"));
    }

    #[test]
    fn build_ai_native_sound_reference_renders_shape_clip() {
        let seed = 4242;
        let difficulty = Difficulty::Easy;
        let scene_index = 0;
        let (_config, scene) = build_config_and_scene(seed, difficulty, scene_index)
            .expect("scene should build");
        assert!(!scene.shape_paths.is_empty());

        let identity = shape_identity_for_scene(&scene, 0).expect("first shape identity should build");
        let bytes = build_ai_native_sound_reference(
            seed,
            difficulty,
            scene_index,
            &identity.shape_id,
        )
        .expect("sound reference should build");

        let reader = hound::WavReader::new(Cursor::new(bytes))
            .expect("sound reference WAV should be parseable");
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_format, hound::SampleFormat::Int);
    }

    #[test]
    fn build_ai_native_sound_reference_rejects_unknown_shape() {
        let seed = 4242;
        let difficulty = Difficulty::Easy;
        let scene_index = 0;

        let err = build_ai_native_sound_reference(seed, difficulty, scene_index, "not_a_shape")
            .expect_err("unknown shape id should fail");
        assert!(err.to_string().contains("is not present in this scene"));
    }
}
