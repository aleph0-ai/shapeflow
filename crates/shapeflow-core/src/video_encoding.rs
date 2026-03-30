use crate::config::EasingFamily;
use crate::scene_generation::{MotionEvent, SceneGenerationOutput};
use crate::tabular_encoding::shape_identity_for_index;
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder, Rgb, RgbImage};

const KEYFRAME_BORDER_COLOR: Rgb<u8> = Rgb([220, 20, 60]);

#[derive(Debug, thiserror::Error)]
pub enum VideoEncodingError {
    #[error("resolution must be > 0")]
    InvalidResolution,
    #[error("shape identity generation failed: {0}")]
    ShapeIdentity(String),
    #[error("shape index {shape_index} out of bounds for scene with {shape_count} shapes")]
    ShapeIndexOutOfBounds {
        shape_index: usize,
        shape_count: usize,
    },
    #[error("shape {shape_index} is missing an initial position in scene data")]
    MissingInitialPosition { shape_index: usize },
    #[error("motion event {global_event_index} has zero frame duration")]
    InvalidEventDuration { global_event_index: u32 },
    #[error(
        "time_slot {time_slot} has inconsistent durations: expected {expected}, event {global_event_index} has {found}"
    )]
    MismatchedSlotDuration {
        time_slot: u32,
        expected: u16,
        found: u16,
        global_event_index: u32,
    },
    #[error("failed to encode PNG frame: {0}")]
    PngEncoding(String),
}

pub fn render_scene_video_frames_png(
    scene: &SceneGenerationOutput,
    resolution: u32,
) -> Result<Vec<Vec<u8>>, VideoEncodingError> {
    render_scene_video_frames_png_with_keyframe_border(scene, resolution, false)
}

pub fn render_scene_video_frames_png_with_keyframe_border(
    scene: &SceneGenerationOutput,
    resolution: u32,
    draw_keyframe_border: bool,
) -> Result<Vec<Vec<u8>>, VideoEncodingError> {
    if resolution == 0 {
        return Err(VideoEncodingError::InvalidResolution);
    }

    let shape_count = scene.shape_paths.len();
    let mut shape_colors = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        let identity = shape_identity_for_index(shape_index)
            .map_err(|error| VideoEncodingError::ShapeIdentity(error.to_string()))?;
        shape_colors.push(color_name_to_rgb(identity.color.as_str()));
    }

    let mut current_positions = vec![(0.0_f64, 0.0_f64); shape_count];
    let mut initialized = vec![false; shape_count];
    for event in &scene.motion_events {
        if event.shape_index >= shape_count {
            return Err(VideoEncodingError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        if event.duration_frames == 0 {
            return Err(VideoEncodingError::InvalidEventDuration {
                global_event_index: event.global_event_index,
            });
        }
        if !initialized[event.shape_index] {
            current_positions[event.shape_index] = (event.start_point.x, event.start_point.y);
            initialized[event.shape_index] = true;
        }
    }
    for shape in &scene.shape_paths {
        if shape.shape_index >= shape_count {
            return Err(VideoEncodingError::ShapeIndexOutOfBounds {
                shape_index: shape.shape_index,
                shape_count,
            });
        }
        if !initialized[shape.shape_index] {
            if let Some(point) = shape.trajectory_points.first() {
                current_positions[shape.shape_index] = (point.x, point.y);
                initialized[shape.shape_index] = true;
            }
        }
    }
    for (shape_index, has_position) in initialized.into_iter().enumerate() {
        if !has_position {
            return Err(VideoEncodingError::MissingInitialPosition { shape_index });
        }
    }

    if scene.motion_events.is_empty() {
        return Ok(Vec::new());
    }

    let max_time_slot = scene
        .motion_events
        .iter()
        .map(|event| event.time_slot)
        .max()
        .unwrap_or(0);
    let mut events_by_slot = vec![Vec::<&MotionEvent>::new(); (max_time_slot as usize) + 1];
    for event in &scene.motion_events {
        events_by_slot[event.time_slot as usize].push(event);
    }

    let mut frame_pngs = Vec::new();
    for (slot_index, slot_events) in events_by_slot.iter_mut().enumerate() {
        if slot_events.is_empty() {
            continue;
        }
        slot_events.sort_by_key(|event| event.global_event_index);
        let expected_duration = slot_events[0].duration_frames;
        for event in slot_events.iter().skip(1) {
            if event.duration_frames != expected_duration {
                return Err(VideoEncodingError::MismatchedSlotDuration {
                    time_slot: slot_index as u32,
                    expected: expected_duration,
                    found: event.duration_frames,
                    global_event_index: event.global_event_index,
                });
            }
        }

        let frame_count = usize::from(expected_duration);
        for frame_index in 0..frame_count {
            let t = normalized_progress(frame_index, frame_count);
            let mut frame_positions = current_positions.clone();
            for event in slot_events.iter() {
                let eased = easing_progress(t, event.easing);
                frame_positions[event.shape_index] = (
                    lerp(event.start_point.x, event.end_point.x, eased),
                    lerp(event.start_point.y, event.end_point.y, eased),
                );
            }

            frame_pngs.push(render_frame_png(
                &frame_positions,
                &shape_colors,
                resolution,
                draw_keyframe_border && frame_index + 1 == frame_count,
            )?);
        }

        for event in slot_events.iter() {
            current_positions[event.shape_index] = (event.end_point.x, event.end_point.y);
        }
    }

    Ok(frame_pngs)
}

fn render_frame_png(
    positions: &[(f64, f64)],
    shape_colors: &[Rgb<u8>],
    resolution: u32,
    draw_keyframe_border: bool,
) -> Result<Vec<u8>, VideoEncodingError> {
    let mut canvas = RgbImage::from_pixel(resolution, resolution, Rgb([255, 255, 255]));
    draw_axes(&mut canvas);
    for (shape_index, (x, y)) in positions.iter().copied().enumerate() {
        let center = normalized_to_pixel(x, y, resolution);
        draw_filled_circle(
            &mut canvas,
            center.0,
            center.1,
            marker_radius(resolution),
            shape_colors[shape_index],
        );
    }
    if draw_keyframe_border {
        draw_keyframe_border_outline(&mut canvas, keyframe_border_width(resolution));
    }

    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(
            canvas.as_raw(),
            resolution,
            resolution,
            ColorType::Rgb8.into(),
        )
        .map_err(|error| VideoEncodingError::PngEncoding(error.to_string()))?;
    Ok(encoded)
}

fn keyframe_border_width(resolution: u32) -> u32 {
    (resolution.saturating_mul(3) / 100).max(1)
}

fn draw_keyframe_border_outline(canvas: &mut RgbImage, border_width: u32) {
    let width = canvas.width();
    let height = canvas.height();
    let layers = border_width.min(width).min(height);
    for offset in 0..layers {
        let left = offset;
        let right = width - 1 - offset;
        let top = offset;
        let bottom = height - 1 - offset;

        for x in left..=right {
            canvas.put_pixel(x, top, KEYFRAME_BORDER_COLOR);
            canvas.put_pixel(x, bottom, KEYFRAME_BORDER_COLOR);
        }
        for y in top..=bottom {
            canvas.put_pixel(left, y, KEYFRAME_BORDER_COLOR);
            canvas.put_pixel(right, y, KEYFRAME_BORDER_COLOR);
        }
    }
}

fn normalized_progress(frame_index: usize, frame_count: usize) -> f64 {
    if frame_count <= 1 {
        return 1.0;
    }
    frame_index as f64 / (frame_count.saturating_sub(1)) as f64
}

fn easing_progress(t: f64, easing: EasingFamily) -> f64 {
    match easing {
        EasingFamily::Linear => t,
        EasingFamily::EaseIn => t * t,
        EasingFamily::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        EasingFamily::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - ((-2.0 * t + 2.0) * (-2.0 * t + 2.0)) / 2.0
            }
        }
    }
}

fn lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

fn draw_axes(canvas: &mut RgbImage) {
    let width = canvas.width() as i32;
    let height = canvas.height() as i32;
    let center_x = width / 2;
    let center_y = height / 2;
    let axis = Rgb([0, 0, 0]);

    draw_line(canvas, 0, center_y, width - 1, center_y, axis);
    draw_line(canvas, center_x, 0, center_x, height - 1, axis);
}

fn marker_radius(resolution: u32) -> i32 {
    (resolution / 96).max(3) as i32
}

fn normalized_to_pixel(x: f64, y: f64, resolution: u32) -> (i32, i32) {
    let max = (resolution.saturating_sub(1)) as f64;
    let px = (((x + 1.0) * 0.5) * max).round().clamp(0.0, max) as i32;
    let py = (((1.0 - (y + 1.0) * 0.5) * max).round()).clamp(0.0, max) as i32;
    (px, py)
}

fn color_name_to_rgb(color: &str) -> Rgb<u8> {
    match color {
        "red" => Rgb([220, 20, 60]),
        "blue" => Rgb([30, 90, 220]),
        "green" => Rgb([30, 160, 60]),
        "yellow" => Rgb([230, 190, 40]),
        "orange" => Rgb([230, 120, 20]),
        "purple" => Rgb([130, 60, 170]),
        "white" => Rgb([200, 200, 200]),
        "cyan" => Rgb([20, 170, 190]),
        _ => Rgb([60, 60, 60]),
    }
}

fn draw_line(canvas: &mut RgbImage, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: Rgb<u8>) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        set_pixel_if_in_bounds(canvas, x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_filled_circle(
    canvas: &mut RgbImage,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: Rgb<u8>,
) {
    let radius_sq = radius * radius;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if (dx * dx) + (dy * dy) <= radius_sq {
                set_pixel_if_in_bounds(canvas, center_x + dx, center_y + dy, color);
            }
        }
    }
}

fn set_pixel_if_in_bounds(canvas: &mut RgbImage, x: i32, y: i32, color: Rgb<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let width = canvas.width() as i32;
    let height = canvas.height() as i32;
    if x >= width || y >= height {
        return;
    }
    canvas.put_pixel(x as u32, y as u32, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_generation::{
        MotionEventAccounting, SceneGenerationParams, SceneProjectionMode, SceneShapePath,
        generate_scene,
    };
    use crate::seed_schedule::SceneSeedSchedule;
    use crate::{MotionEvent, NormalizedPoint, ShapeFlowConfig};

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config must parse")
    }

    #[test]
    fn rendered_video_frames_are_deterministic() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 2,
            samples_per_event: 24,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let first = render_scene_video_frames_png(&scene, config.scene.resolution)
            .expect("video frames should render");
        let second = render_scene_video_frames_png(&scene, config.scene.resolution)
            .expect("video frames should render");
        assert_eq!(first, second);
        assert!(!first.is_empty());

        let expected_frame_count = usize::try_from(
            scene
                .motion_events
                .iter()
                .map(|event| event.time_slot)
                .max()
                .unwrap_or(0)
                + 1,
        )
        .unwrap_or(0)
            * usize::from(config.scene.event_duration_frames);
        assert_eq!(first.len(), expected_frame_count);

        let decoded = image::load_from_memory(&first[0]).expect("first frame should decode");
        assert_eq!(decoded.width(), config.scene.resolution);
        assert_eq!(decoded.height(), config.scene.resolution);
    }

    #[test]
    fn render_fails_on_invalid_shape_index() {
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
                shape_index: 2,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                end_point: NormalizedPoint::new(0.1, 0.1).expect("point must build"),
                duration_frames: 24,
                easing: EasingFamily::Linear,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1],
                generated_per_shape: vec![1],
            },
        };

        let error =
            render_scene_video_frames_png(&scene, 64).expect_err("invalid scene should fail");
        assert!(matches!(
            error,
            VideoEncodingError::ShapeIndexOutOfBounds {
                shape_index: 2,
                shape_count: 1,
            }
        ));
    }

    #[test]
    fn render_fails_on_zero_duration_event() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_paths: vec![SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![NormalizedPoint::new(0.0, 0.0).expect("point must build")],
                soft_memberships: None,
            }],
            motion_events: vec![MotionEvent {
                global_event_index: 7,
                time_slot: 0,
                shape_index: 0,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                end_point: NormalizedPoint::new(0.1, 0.1).expect("point must build"),
                duration_frames: 0,
                easing: EasingFamily::Linear,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1],
                generated_per_shape: vec![1],
            },
        };

        let error =
            render_scene_video_frames_png(&scene, 64).expect_err("zero-duration event should fail");
        assert!(matches!(
            error,
            VideoEncodingError::InvalidEventDuration {
                global_event_index: 7,
            }
        ));
    }

    #[test]
    fn render_does_not_draw_keyframe_border_when_disabled() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_paths: vec![SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![
                    NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                    NormalizedPoint::new(0.2, 0.2).expect("point must build"),
                ],
                soft_memberships: None,
            }],
            motion_events: vec![MotionEvent {
                global_event_index: 0,
                time_slot: 0,
                shape_index: 0,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                end_point: NormalizedPoint::new(0.2, 0.2).expect("point must build"),
                duration_frames: 2,
                easing: EasingFamily::Linear,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1],
                generated_per_shape: vec![1],
            },
        };

        let frames = render_scene_video_frames_png_with_keyframe_border(&scene, 64, false)
            .expect("render should succeed");
        assert_eq!(frames.len(), 2);
        let keyframe = image::load_from_memory(&frames[1])
            .expect("frame should decode")
            .to_rgb8();
        assert_ne!(*keyframe.get_pixel(0, 0), KEYFRAME_BORDER_COLOR);
    }

    #[test]
    fn render_draws_keyframe_border_only_on_keyframe_frames_when_enabled() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_paths: vec![SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![
                    NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                    NormalizedPoint::new(0.2, 0.2).expect("point must build"),
                ],
                soft_memberships: None,
            }],
            motion_events: vec![MotionEvent {
                global_event_index: 0,
                time_slot: 0,
                shape_index: 0,
                shape_event_index: 0,
                start_point: NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                end_point: NormalizedPoint::new(0.2, 0.2).expect("point must build"),
                duration_frames: 2,
                easing: EasingFamily::Linear,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1],
                generated_per_shape: vec![1],
            },
        };

        let frames = render_scene_video_frames_png_with_keyframe_border(&scene, 64, true)
            .expect("render should succeed");
        assert_eq!(frames.len(), 2);

        let first = image::load_from_memory(&frames[0])
            .expect("frame should decode")
            .to_rgb8();
        let last = image::load_from_memory(&frames[1])
            .expect("frame should decode")
            .to_rgb8();
        assert_ne!(*first.get_pixel(0, 0), KEYFRAME_BORDER_COLOR);
        assert_eq!(*last.get_pixel(0, 0), KEYFRAME_BORDER_COLOR);
    }
}
