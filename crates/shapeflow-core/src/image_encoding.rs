use crate::scene_generation::SceneGenerationOutput;
use crate::tabular_encoding::shape_identity_for_index;
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder, Rgb, RgbImage};

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
    let scaled = (resolution / 128).max(2);
    scaled as i32
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
        MotionEvent, MotionEventAccounting, SceneGenerationParams, SceneProjectionMode,
        SceneShapePath, generate_scene,
    };
    use crate::seed_schedule::SceneSeedSchedule;
    use crate::{NormalizedPoint, ShapeFlowConfig};

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
