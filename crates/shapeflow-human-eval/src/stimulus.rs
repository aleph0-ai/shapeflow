use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gif::{Encoder, Frame, Repeat};
use shapeflow_core::{
    EasingFamily, MotionEvent, MotionEventAccounting, NormalizedPoint, SceneGenerationOutput,
    ShapeFlowConfig,
    image_encoding::render_scene_image_png_with_scene_config,
    sound_encoding::render_scene_sound_wav,
    tabular_encoding::{
        generate_tabular_motion_rows, serialize_tabular_motion_rows_csv, shape_identity_for_scene,
    },
    text_encoding::{generate_scene_text_lines_with_scene_config, serialize_scene_text},
    video_encoding::render_scene_video_frames_png_with_keyframe_border,
};

use crate::flow::{self, Difficulty, Modality, PlanItem};

const MAX_VIDEO_PREVIEW_RESOLUTION: u32 = 256;
const MAX_VIDEO_PLAYER_FRAMES: usize = 180;
const MAX_VIDEO_GIF_FRAMES: usize = 120;

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
            let lines = generate_scene_text_lines_with_scene_config(&scene, &config.scene)
                .map_err(|error| anyhow::anyhow!("{error}"))
                .context("failed to render text stimulus")?;
            Ok(TaskStimulus::Text {
                body: serialize_scene_text(&lines),
            })
        }
        Modality::Tabular => {
            let rows = generate_tabular_motion_rows(&scene)
                .map_err(|error| anyhow::anyhow!("{error}"))
                .context("failed to render tabular stimulus rows")?;
            Ok(TaskStimulus::TabularCsv {
                csv: serialize_tabular_motion_rows_csv(&rows),
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
            let lines = generate_scene_text_lines_with_scene_config(&scene, &config.scene)
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
                text: serialize_tabular_motion_rows_csv(&rows),
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

fn build_config_and_scene(
    seed: u64,
    difficulty: Difficulty,
    scene_index: u32,
) -> Result<(ShapeFlowConfig, SceneGenerationOutput)> {
    let config = flow::build_session_config(seed, difficulty)
        .map_err(|error| anyhow::anyhow!("{error}"))
        .context("failed to build session config")?;
    let scene = flow::build_scene_for_index(&config, scene_index)
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
