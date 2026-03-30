use crate::config::{ImageArrowType, SceneConfig};
use crate::scene_generation::{MotionEvent, SceneGenerationOutput};
use crate::tabular_encoding::shape_identity_for_index;
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder, Rgb, RgbImage};
use rand::RngCore;
use rand_chacha::ChaCha8Rng;

#[derive(Debug, thiserror::Error)]
pub enum ImageEncodingError {
    #[error("resolution must be > 0")]
    InvalidResolution,
    #[error("shape identity generation failed: {0}")]
    ShapeIdentity(String),
    #[error("shape index {shape_index} out of bounds for scene with {shape_count} shapes")]
    ShapeIndexOutOfBounds {
        shape_index: usize,
        shape_count: usize,
    },
    #[error("failed to encode PNG image: {0}")]
    PngEncoding(String),
}

pub fn render_scene_image_png(
    scene: &SceneGenerationOutput,
    resolution: u32,
) -> Result<Vec<u8>, ImageEncodingError> {
    if resolution == 0 {
        return Err(ImageEncodingError::InvalidResolution);
    }

    let shape_count = scene.shape_paths.len();
    let mut shape_colors = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        let identity = shape_identity_for_index(shape_index)
            .map_err(|error| ImageEncodingError::ShapeIdentity(error.to_string()))?;
        shape_colors.push(color_name_to_rgb(identity.color.as_str()));
    }

    let mut canvas = RgbImage::from_pixel(resolution, resolution, Rgb([255, 255, 255]));
    draw_axes(&mut canvas);

    for event in &scene.motion_events {
        if event.shape_index >= shape_count {
            return Err(ImageEncodingError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }

        let color = shape_colors[event.shape_index];
        let start = normalized_to_pixel(event.start_point.x, event.start_point.y, resolution);
        let end = normalized_to_pixel(event.end_point.x, event.end_point.y, resolution);
        draw_line(&mut canvas, start.0, start.1, end.0, end.1, color);
        draw_filled_circle(&mut canvas, end.0, end.1, marker_radius(resolution), color);
    }

    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(
            canvas.as_raw(),
            resolution,
            resolution,
            ColorType::Rgb8.into(),
        )
        .map_err(|error| ImageEncodingError::PngEncoding(error.to_string()))?;
    Ok(encoded)
}

pub fn render_scene_image_png_with_scene_config(
    scene: &SceneGenerationOutput,
    scene_cfg: &SceneConfig,
) -> Result<Vec<u8>, ImageEncodingError> {
    if scene_cfg.resolution == 0 {
        return Err(ImageEncodingError::InvalidResolution);
    }

    let shape_count = scene.shape_paths.len();
    let mut shape_colors = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        let identity = shape_identity_for_index(shape_index)
            .map_err(|error| ImageEncodingError::ShapeIdentity(error.to_string()))?;
        shape_colors.push(color_name_to_rgb(identity.color.as_str()));
    }

    let thumbnail_size = (scene_cfg.resolution / 3).max(1);
    let canvas_size = scene_cfg
        .resolution
        .checked_mul(3)
        .ok_or(ImageEncodingError::InvalidResolution)?;
    let mut canvas = RgbImage::from_pixel(canvas_size, canvas_size, Rgb([255, 255, 255]));

    let placements = build_thumbnail_placements(scene, scene_cfg, thumbnail_size);
    let mut frame_infos = Vec::with_capacity(scene.motion_events.len());

    for (event_index, event) in scene.motion_events.iter().enumerate() {
        if event.shape_index >= shape_count {
            return Err(ImageEncodingError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        let color = shape_colors[event.shape_index];
        let placement = placements[event_index];
        let frame_info = render_motion_frame(&mut canvas, event, placement, thumbnail_size, color);
        frame_infos.push(frame_info);
    }

    draw_connectors(&mut canvas, scene_cfg.image_arrow_type, &frame_infos);

    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(
            canvas.as_raw(),
            canvas_size,
            canvas_size,
            ColorType::Rgb8.into(),
        )
        .map_err(|error| ImageEncodingError::PngEncoding(error.to_string()))?;
    Ok(encoded)
}

#[derive(Clone, Copy)]
struct ThumbnailPlacement {
    origin_x: i32,
    origin_y: i32,
}

#[derive(Clone, Copy)]
struct ThumbnailRenderInfo {
    anchor: (i32, i32),
    start: (i32, i32),
    end: (i32, i32),
}

fn build_thumbnail_placements(
    scene: &SceneGenerationOutput,
    scene_cfg: &SceneConfig,
    thumbnail_size: u32,
) -> Vec<ThumbnailPlacement> {
    let event_count = scene.motion_events.len();
    let mut placements = Vec::with_capacity(event_count);
    if event_count == 0 {
        return placements;
    }

    let cell_size = i64::from(scene_cfg.resolution);
    let thumbnail_size = i64::from(thumbnail_size);
    let max_jitter = (cell_size - thumbnail_size).max(0);

    let mut slots: Vec<usize> = (0..9).collect();
    let mut rng = scene.schedule.scene_layout_rng();
    if scene_cfg.image_frame_scatter {
        shuffle_slots(&mut rng, &mut slots);
    }

    for event_index in 0..event_count {
        let slot = if scene_cfg.image_frame_scatter {
            slots[event_index % slots.len()]
        } else {
            event_index % 9
        };

        let col = i64::from((slot % 3) as u32);
        let row = i64::from((slot / 3) as u32);
        let mut origin_x = col * cell_size;
        let mut origin_y = row * cell_size;

        if scene_cfg.image_frame_scatter && max_jitter > 0 {
            origin_x += bounded_random_offset(&mut rng, max_jitter);
            origin_y += bounded_random_offset(&mut rng, max_jitter);
        }

        placements.push(ThumbnailPlacement {
            origin_x: i32::try_from(origin_x).expect("thumbnail origin x must fit i32"),
            origin_y: i32::try_from(origin_y).expect("thumbnail origin y must fit i32"),
        });
    }

    placements
}

fn render_motion_frame(
    canvas: &mut RgbImage,
    event: &MotionEvent,
    placement: ThumbnailPlacement,
    thumbnail_size: u32,
    color: Rgb<u8>,
) -> ThumbnailRenderInfo {
    let thumbnail_size_i32 = i32::try_from(thumbnail_size).expect("thumbnail size must fit i32");
    fill_rect(
        canvas,
        placement.origin_x,
        placement.origin_y,
        thumbnail_size_i32,
        thumbnail_size_i32,
        Rgb([255, 255, 255]),
    );
    draw_axes_in_box(
        canvas,
        placement.origin_x,
        placement.origin_y,
        thumbnail_size_i32,
    );

    let start_local = normalized_to_pixel(event.start_point.x, event.start_point.y, thumbnail_size);
    let end_local = normalized_to_pixel(event.end_point.x, event.end_point.y, thumbnail_size);

    let start = (
        placement.origin_x + start_local.0,
        placement.origin_y + start_local.1,
    );
    let end = (
        placement.origin_x + end_local.0,
        placement.origin_y + end_local.1,
    );
    let anchor = (
        placement.origin_x + thumbnail_size_i32 / 2,
        placement.origin_y + thumbnail_size_i32 / 2,
    );

    draw_line(canvas, start.0, start.1, end.0, end.1, color);
    draw_filled_circle(
        canvas,
        end.0,
        end.1,
        marker_radius_for_size(thumbnail_size),
        color,
    );

    ThumbnailRenderInfo { anchor, start, end }
}

fn draw_connectors(
    canvas: &mut RgbImage,
    arrow_type: ImageArrowType,
    frame_infos: &[ThumbnailRenderInfo],
) {
    if frame_infos.is_empty() {
        return;
    }

    let color = Rgb([0, 0, 0]);
    match arrow_type {
        ImageArrowType::Prev => {
            for i in 1..frame_infos.len() {
                let src = frame_infos[i - 1].anchor;
                let dst = frame_infos[i].anchor;
                draw_arrow(canvas, src.0, src.1, dst.0, dst.1, color);
            }
        }
        ImageArrowType::Current => {
            for frame in frame_infos {
                draw_arrow(
                    canvas,
                    frame.start.0,
                    frame.start.1,
                    frame.end.0,
                    frame.end.1,
                    color,
                );
            }
        }
        ImageArrowType::Next => {
            for i in 0..frame_infos.len() - 1 {
                let src = frame_infos[i].anchor;
                let dst = frame_infos[i + 1].anchor;
                draw_arrow(canvas, src.0, src.1, dst.0, dst.1, color);
            }
        }
    }
}

fn shuffle_slots(rng: &mut ChaCha8Rng, slots: &mut [usize]) {
    for i in (1..slots.len()).rev() {
        let upper = u64::try_from(i + 1).expect("upper bound should fit u64");
        let j = usize::try_from(rng.next_u64() % upper).expect("shuffle index should fit usize");
        slots.swap(i, j);
    }
}

fn bounded_random_offset(rng: &mut ChaCha8Rng, max_exclusive: i64) -> i64 {
    if max_exclusive <= 0 {
        return 0;
    }
    let max_u64 =
        u64::try_from(max_exclusive).expect("non-negative jitter range should convert to u64");
    let offset = rng.next_u64() % max_u64;
    i64::try_from(offset).expect("jitter offset should fit i64")
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

fn draw_axes_in_box(canvas: &mut RgbImage, origin_x: i32, origin_y: i32, size: i32) {
    let axis = Rgb([0, 0, 0]);
    let min_x = origin_x;
    let min_y = origin_y;
    let max_x = origin_x + size - 1;
    let max_y = origin_y + size - 1;
    let center_x = (origin_x + max_x) / 2;
    let center_y = (origin_y + max_y) / 2;

    draw_line(canvas, min_x, center_y, max_x, center_y, axis);
    draw_line(canvas, center_x, min_y, center_x, max_y, axis);
}

fn marker_radius(resolution: u32) -> i32 {
    let scaled = (resolution / 128).max(2);
    scaled as i32
}

fn marker_radius_for_size(size: u32) -> i32 {
    let scaled = (size / 32).max(2);
    scaled as i32
}

fn fill_rect(canvas: &mut RgbImage, x: i32, y: i32, width: i32, height: i32, color: Rgb<u8>) {
    for dy in 0..height {
        for dx in 0..width {
            set_pixel_if_in_bounds(canvas, x + dx, y + dy, color);
        }
    }
}

fn normalized_to_pixel(x: f64, y: f64, resolution: u32) -> (i32, i32) {
    let max = (resolution.saturating_sub(1)) as f64;
    let px = (((x + 1.0) * 0.5) * max).round().clamp(0.0, max) as i32;
    let py = (((1.0 - (y + 1.0) * 0.5) * max).round()).clamp(0.0, max) as i32;
    (px, py)
}

fn draw_arrow(canvas: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb<u8>) {
    draw_line(canvas, x0, y0, x1, y1, color);

    let dx = (x1 - x0) as f64;
    let dy = (y1 - y0) as f64;
    let length = (dx * dx + dy * dy).sqrt();
    if length == 0.0 {
        return;
    }

    let unit_x = dx / length;
    let unit_y = dy / length;
    let arrow_size = 8.0_f64.min(length / 2.0);
    let theta = core::f64::consts::FRAC_PI_6;
    let cos_theta = theta.cos();
    let sin_theta = theta.sin();
    let tip_x = x1 as f64;
    let tip_y = y1 as f64;

    let left_x = tip_x - arrow_size * (unit_x * cos_theta - unit_y * sin_theta);
    let left_y = tip_y - arrow_size * (unit_x * sin_theta + unit_y * cos_theta);
    let right_x = tip_x - arrow_size * (unit_x * cos_theta + unit_y * sin_theta);
    let right_y = tip_y - arrow_size * (-unit_x * sin_theta + unit_y * cos_theta);

    draw_line(
        canvas,
        x1,
        y1,
        left_x.round() as i32,
        left_y.round() as i32,
        color,
    );
    draw_line(
        canvas,
        x1,
        y1,
        right_x.round() as i32,
        right_y.round() as i32,
        color,
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_generation::{
        MotionEvent, MotionEventAccounting, SceneGenerationParams, SceneProjectionMode,
        SceneShapePath, generate_scene,
    };
    use crate::seed_schedule::SceneSeedSchedule;
    use crate::{ShapeFlowConfig, config::ImageArrowType};

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config must parse")
    }

    #[test]
    fn rendered_image_png_is_deterministic() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 1,
            samples_per_event: 24,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let first = render_scene_image_png(&scene, config.scene.resolution)
            .expect("image generation should succeed");
        let second = render_scene_image_png(&scene, config.scene.resolution)
            .expect("image generation should succeed");
        assert_eq!(first, second);
        assert!(!first.is_empty());

        let decoded = image::load_from_memory(&first).expect("generated image should decode");
        assert_eq!(decoded.width(), config.scene.resolution);
        assert_eq!(decoded.height(), config.scene.resolution);
    }

    #[test]
    fn rendered_image_png_with_scene_config_is_deterministic_and_scaled() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 1,
            samples_per_event: 24,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let first = render_scene_image_png_with_scene_config(&scene, &config.scene)
            .expect("image generation should succeed");
        let second = render_scene_image_png_with_scene_config(&scene, &config.scene)
            .expect("image generation should succeed");
        assert_eq!(first, second);
        assert!(!first.is_empty());

        let decoded = image::load_from_memory(&first).expect("generated image should decode");
        assert_eq!(decoded.width(), config.scene.resolution * 3);
        assert_eq!(decoded.height(), config.scene.resolution * 3);
    }

    #[test]
    fn scene_image_rendering_depends_on_scatter_setting() {
        let mut cfg_a = bootstrap_config();
        let mut cfg_b = bootstrap_config();
        cfg_a.scene.image_frame_scatter = false;
        cfg_b.scene.image_frame_scatter = true;

        let params = SceneGenerationParams {
            config: &cfg_a,
            scene_index: 7,
            samples_per_event: 16,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let without_scatter = render_scene_image_png_with_scene_config(&scene, &cfg_a.scene)
            .expect("image generation should succeed");
        let with_scatter = render_scene_image_png_with_scene_config(&scene, &cfg_b.scene)
            .expect("image generation should succeed");
        assert_ne!(without_scatter, with_scatter);
    }

    #[test]
    fn scene_image_rendering_depends_on_arrow_type() {
        let mut cfg_a = bootstrap_config();
        let mut cfg_b = bootstrap_config();
        cfg_a.scene.image_arrow_type = ImageArrowType::Current;
        cfg_b.scene.image_arrow_type = ImageArrowType::Next;

        let params = SceneGenerationParams {
            config: &cfg_a,
            scene_index: 11,
            samples_per_event: 16,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let current = render_scene_image_png_with_scene_config(&scene, &cfg_a.scene)
            .expect("image generation should succeed");
        let next = render_scene_image_png_with_scene_config(&scene, &cfg_b.scene)
            .expect("image generation should succeed");
        assert_ne!(current, next);
    }

    #[test]
    fn render_fails_on_invalid_shape_index() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_paths: vec![SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![
                    crate::NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                ],
                soft_memberships: None,
            }],
            motion_events: vec![MotionEvent {
                global_event_index: 0,
                time_slot: 0,
                shape_index: 1,
                shape_event_index: 0,
                start_point: crate::NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                end_point: crate::NormalizedPoint::new(0.1, 0.1).expect("point must build"),
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

        let error = render_scene_image_png(&scene, 64).expect_err("invalid scene should fail");
        assert!(matches!(
            error,
            ImageEncodingError::ShapeIndexOutOfBounds {
                shape_index: 1,
                shape_count: 1,
            }
        ));
    }
}
