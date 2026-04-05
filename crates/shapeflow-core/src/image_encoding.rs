use crate::config::{ImageArrowType, SceneConfig};
use crate::scene_generation::SceneGenerationOutput;
use crate::tabular_encoding::shape_identity_for_scene;
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder, Rgb, RgbImage};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

const AXIS_STROKE: i32 = 4;
const COMIC_AXIS_OVERLAY_STROKE: i32 = 1;
const PANEL_BORDER_STROKE: i32 = 4;
const TRAIL_STROKE: i32 = 4;
const CONNECTOR_STROKE: i32 = 4;
const SHAPE_OUTLINE_STROKE: i32 = 1;
const PANEL_DIAGONAL_GUIDE_COLOR: [u8; 3] = [220, 220, 220];
const PANEL_GUTTER_FRACTION: u32 = 8;
const PANEL_PADDING_FRACTION: u32 = 10;
const ARROW_HEAD_MIN_SIZE: f64 = 12.0;
const ARROW_HEAD_SIZE_MULTIPLIER: f64 = 4.5;
const COMIC_CORNER_MIX_SEED: u64 = 0xC0DE_F00D_5EED_CAFE;

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
    let mut shape_styles = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        let identity = shape_identity_for_scene(scene, shape_index)
            .map_err(|error| ImageEncodingError::ShapeIdentity(error.to_string()))?;
        shape_styles.push((
            color_name_to_rgb(identity.color.as_str()),
            identity.shape_type,
        ));
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

        let (color, shape_type) = &shape_styles[event.shape_index];
        let start = normalized_to_pixel(event.start_point.x, event.start_point.y, resolution);
        let end = normalized_to_pixel(event.end_point.x, event.end_point.y, resolution);
        draw_line_thick(
            &mut canvas,
            start.0,
            start.1,
            end.0,
            end.1,
            *color,
            TRAIL_STROKE,
        );
        draw_shape_geometry(
            &mut canvas,
            end.0,
            end.1,
            marker_radius_for_shape(marker_radius(resolution), shape_type),
            *color,
            shape_type,
        );
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
    let mut shape_styles = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        let identity = shape_identity_for_scene(scene, shape_index)
            .map_err(|error| ImageEncodingError::ShapeIdentity(error.to_string()))?;
        shape_styles.push((
            color_name_to_rgb(identity.color.as_str()),
            identity.shape_type,
        ));
    }

    let slot_panels = build_time_slot_panels(scene, scene_cfg.n_motion_slots);
    let panel_count = slot_panels.len();
    let cols = compute_layout_cols(panel_count);
    let rows = compute_layout_rows(panel_count, cols);
    let thumbnail_size = compute_thumbnail_size(panel_count, scene_cfg.resolution);
    let gutter = compute_panel_gutter(thumbnail_size);
    let canvas_width = compute_canvas_dimension(cols, thumbnail_size, gutter);
    let canvas_height = compute_canvas_dimension(rows, thumbnail_size, gutter);
    let mut canvas = RgbImage::from_pixel(canvas_width, canvas_height, Rgb([255, 255, 255]));

    if panel_count == 0 {
        let mut encoded = Vec::new();
        PngEncoder::new(&mut encoded)
            .write_image(
                canvas.as_raw(),
                canvas_width,
                canvas_height,
                ColorType::Rgb8.into(),
            )
            .map_err(|error| ImageEncodingError::PngEncoding(error.to_string()))?;
        return Ok(encoded);
    }

    let placements =
        build_thumbnail_placements(panel_count, scene, scene_cfg, thumbnail_size, cols, gutter);
    let padding = (thumbnail_size / PANEL_PADDING_FRACTION).max(1);

    let mut frame_infos = Vec::with_capacity(panel_count);
    for (panel_index, panel) in slot_panels.iter().enumerate() {
        let placement = placements[panel_index];
        render_time_slot_frame_base(&mut canvas, placement, thumbnail_size);
        let frame_info = compute_time_slot_frame_info(
            scene,
            panel,
            &shape_styles,
            shape_count,
            placement,
            thumbnail_size,
            padding,
        )?;
        frame_infos.push(frame_info);
    }

    draw_connectors(&mut canvas, scene_cfg.image_arrow_type, &frame_infos);

    for (panel_index, panel) in slot_panels.iter().enumerate() {
        let placement = placements[panel_index];
        render_time_slot_frame_motion_and_axes(
            &mut canvas,
            scene,
            panel,
            &shape_styles,
            shape_count,
            placement,
            thumbnail_size,
            padding,
        )?;
    }

    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(
            canvas.as_raw(),
            canvas_width,
            canvas_height,
            ColorType::Rgb8.into(),
        )
        .map_err(|error| ImageEncodingError::PngEncoding(error.to_string()))?;
    Ok(encoded)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ThumbnailPlacement {
    origin_x: i32,
    origin_y: i32,
}

#[derive(Clone)]
struct ThumbnailRenderInfo {
    anchor: (i32, i32),
    segments: Vec<MotionSegmentInfo>,
}

#[derive(Clone, Copy)]
struct MotionSegmentInfo {
    start: (i32, i32),
    end: (i32, i32),
    marker_clearance: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TimeSlotPanel {
    time_slot: u32,
    event_indices: Vec<usize>,
}

fn build_time_slot_panels(
    scene: &SceneGenerationOutput,
    n_motion_slots: u32,
) -> Vec<TimeSlotPanel> {
    let mut grouped: BTreeMap<u32, Vec<usize>> =
        (0..n_motion_slots).map(|slot| (slot, Vec::new())).collect();
    for (event_index, event) in scene.motion_events.iter().enumerate() {
        if let Some(events) = grouped.get_mut(&event.time_slot) {
            events.push(event_index);
        }
    }
    grouped
        .into_iter()
        .map(|(time_slot, event_indices)| TimeSlotPanel {
            time_slot,
            event_indices,
        })
        .collect()
}

fn build_thumbnail_placements(
    panel_count: usize,
    scene: &SceneGenerationOutput,
    scene_cfg: &SceneConfig,
    thumbnail_size: u32,
    cols: u32,
    gutter: u32,
) -> Vec<ThumbnailPlacement> {
    let mut placements = vec![
        ThumbnailPlacement {
            origin_x: 0,
            origin_y: 0,
        };
        panel_count
    ];
    if panel_count == 0 {
        return placements;
    }

    let panel_size = i64::from(thumbnail_size);
    let cols_i64 = i64::from(cols);
    let gutter_i64 = i64::from(gutter);
    let mut order: Vec<usize> = (0..panel_count).collect();
    let mut rng = scene.schedule.scene_layout_rng();
    shuffle_slots(&mut rng, &mut order);
    if order.len() > 1 && is_identity_order(&order) {
        order.rotate_left(1);
    }

    let jitter_max = if scene_cfg.image_frame_scatter {
        i64::try_from(gutter / 3).unwrap_or(0).max(0)
    } else {
        0
    };

    for (slot_index, event_index) in order.iter().copied().enumerate() {
        let col = i64::try_from(slot_index).expect("slot index should fit");
        let row = col / cols_i64;
        let col = col % cols_i64;

        let mut origin_x = col * (panel_size + gutter_i64) + gutter_i64;
        let mut origin_y = row * (panel_size + gutter_i64) + gutter_i64;

        if jitter_max > 0 {
            origin_x += bounded_random_offset(&mut rng, jitter_max);
            origin_y += bounded_random_offset(&mut rng, jitter_max);
        }

        placements[event_index] = ThumbnailPlacement {
            origin_x: i32::try_from(origin_x).expect("thumbnail origin x must fit i32"),
            origin_y: i32::try_from(origin_y).expect("thumbnail origin y must fit i32"),
        };
    }

    placements
}

fn is_identity_order(order: &[usize]) -> bool {
    order
        .iter()
        .copied()
        .enumerate()
        .all(|(index, value)| value == index)
}

#[cfg(test)]
fn render_time_slot_frame(
    canvas: &mut RgbImage,
    scene: &SceneGenerationOutput,
    panel: &TimeSlotPanel,
    shape_styles: &[(Rgb<u8>, String)],
    shape_count: usize,
    placement: ThumbnailPlacement,
    thumbnail_size: u32,
    padding: u32,
) -> Result<ThumbnailRenderInfo, ImageEncodingError> {
    render_time_slot_frame_base(canvas, placement, thumbnail_size);
    let frame_info = compute_time_slot_frame_info(
        scene,
        panel,
        shape_styles,
        shape_count,
        placement,
        thumbnail_size,
        padding,
    )?;
    render_time_slot_frame_motion_and_axes(
        canvas,
        scene,
        panel,
        shape_styles,
        shape_count,
        placement,
        thumbnail_size,
        padding,
    )?;
    Ok(frame_info)
}

fn render_time_slot_frame_base(
    canvas: &mut RgbImage,
    placement: ThumbnailPlacement,
    thumbnail_size: u32,
) {
    let thumbnail_size_i32 = i32::try_from(thumbnail_size).expect("thumbnail size must fit i32");
    fill_rect(
        canvas,
        placement.origin_x,
        placement.origin_y,
        thumbnail_size_i32,
        thumbnail_size_i32,
        Rgb([255, 255, 255]),
    );
    draw_panel_diagonal_guides(
        canvas,
        placement.origin_x,
        placement.origin_y,
        thumbnail_size_i32,
    );

    draw_panel_border(
        canvas,
        placement.origin_x,
        placement.origin_y,
        thumbnail_size_i32,
    );
}

fn compute_time_slot_frame_info(
    scene: &SceneGenerationOutput,
    panel: &TimeSlotPanel,
    shape_styles: &[(Rgb<u8>, String)],
    shape_count: usize,
    placement: ThumbnailPlacement,
    thumbnail_size: u32,
    padding: u32,
) -> Result<ThumbnailRenderInfo, ImageEncodingError> {
    let thumbnail_size_i32 = i32::try_from(thumbnail_size).expect("thumbnail size must fit i32");
    let padding_i32 = i32::try_from(padding).expect("panel padding must fit i32");
    let inner_size = (thumbnail_size_i32 - 2 * padding_i32).max(1);
    let inner_u32 = u32::try_from(inner_size).expect("inner size must fit u32");
    let mut segments = Vec::with_capacity(panel.event_indices.len());

    for &event_index in &panel.event_indices {
        let event = scene
            .motion_events
            .get(event_index)
            .expect("panel event index should be valid");
        if event.shape_index >= shape_count {
            return Err(ImageEncodingError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        let (_, shape_type) = &shape_styles[event.shape_index];
        let start_local = normalized_to_pixel(event.start_point.x, event.start_point.y, inner_u32);
        let end_local = normalized_to_pixel(event.end_point.x, event.end_point.y, inner_u32);
        let start = (
            placement.origin_x + padding_i32 + start_local.0,
            placement.origin_y + padding_i32 + start_local.1,
        );
        let end = (
            placement.origin_x + padding_i32 + end_local.0,
            placement.origin_y + padding_i32 + end_local.1,
        );
        let marker_radius =
            marker_radius_for_shape(marker_radius_for_size(inner_u32), shape_type.as_str());
        segments.push(MotionSegmentInfo {
            start,
            end,
            marker_clearance: marker_radius + 3,
        });
    }

    let inset_x = (thumbnail_size as f64 * 0.10).round() as i32;
    let anchor = connector_panel_anchor(
        scene.schedule.scene_layout,
        panel.time_slot,
        placement,
        thumbnail_size_i32,
        inset_x,
    );

    Ok(ThumbnailRenderInfo { anchor, segments })
}

fn connector_panel_corner(scene_layout_seed: u64, time_slot: u32) -> usize {
    let mixed_seed = scene_layout_seed
        .wrapping_mul(COMIC_CORNER_MIX_SEED)
        .wrapping_add(
            u64::from(time_slot)
                .rotate_left(13)
                .wrapping_add(u64::from(time_slot) << 1),
        );
    let mut rng = ChaCha8Rng::seed_from_u64(mixed_seed);
    let random_corner = usize::try_from(rng.next_u32()).expect("u32 must fit usize") % 4;
    (random_corner + (time_slot as usize % 4)) % 4
}

fn connector_panel_anchor(
    scene_layout_seed: u64,
    time_slot: u32,
    placement: ThumbnailPlacement,
    panel_size: i32,
    inset: i32,
) -> (i32, i32) {
    let corner = connector_panel_corner(scene_layout_seed, time_slot);
    match corner {
        0 => (placement.origin_x + inset, placement.origin_y + inset),
        1 => (
            placement.origin_x + panel_size - inset - 1,
            placement.origin_y + inset,
        ),
        2 => (
            placement.origin_x + inset,
            placement.origin_y + panel_size - inset - 1,
        ),
        _ => (
            placement.origin_x + panel_size - inset - 1,
            placement.origin_y + panel_size - inset - 1,
        ),
    }
}

fn render_time_slot_frame_motion_and_axes(
    canvas: &mut RgbImage,
    scene: &SceneGenerationOutput,
    panel: &TimeSlotPanel,
    shape_styles: &[(Rgb<u8>, String)],
    shape_count: usize,
    placement: ThumbnailPlacement,
    thumbnail_size: u32,
    padding: u32,
) -> Result<(), ImageEncodingError> {
    let thumbnail_size_i32 = i32::try_from(thumbnail_size).expect("thumbnail size must fit i32");
    let padding_i32 = i32::try_from(padding).expect("panel padding must fit i32");
    let inner_size = (thumbnail_size_i32 - 2 * padding_i32).max(1);
    let inner_u32 = u32::try_from(inner_size).expect("inner size must fit u32");

    for &event_index in &panel.event_indices {
        let event = scene
            .motion_events
            .get(event_index)
            .expect("panel event index should be valid");
        if event.shape_index >= shape_count {
            return Err(ImageEncodingError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        let (color, shape_type) = &shape_styles[event.shape_index];
        let start_local = normalized_to_pixel(event.start_point.x, event.start_point.y, inner_u32);
        let end_local = normalized_to_pixel(event.end_point.x, event.end_point.y, inner_u32);
        let start = (
            placement.origin_x + padding_i32 + start_local.0,
            placement.origin_y + padding_i32 + start_local.1,
        );
        let end = (
            placement.origin_x + padding_i32 + end_local.0,
            placement.origin_y + padding_i32 + end_local.1,
        );

        draw_line_thick(canvas, start.0, start.1, end.0, end.1, *color, TRAIL_STROKE);
        let marker_radius =
            marker_radius_for_shape(marker_radius_for_size(inner_u32), shape_type.as_str());
        draw_shape_geometry(
            canvas,
            end.0,
            end.1,
            marker_radius,
            *color,
            shape_type.as_str(),
        );
        let dot_color = inverse_color(*color);
        draw_filled_circle(canvas, end.0, end.1, 1, dot_color);
    }

    let axis_box_size = thumbnail_size_i32;
    draw_axes_in_box(
        canvas,
        placement.origin_x,
        placement.origin_y,
        axis_box_size,
        AXIS_STROKE,
    );
    draw_axes_in_box_with_style(
        canvas,
        placement.origin_x,
        placement.origin_y,
        axis_box_size,
        COMIC_AXIS_OVERLAY_STROKE,
        Rgb([255, 0, 0]),
    );

    Ok(())
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
                draw_arrow(canvas, src.0, src.1, dst.0, dst.1, color, CONNECTOR_STROKE);
            }
        }
        ImageArrowType::Current => {
            for frame in frame_infos {
                for segment in &frame.segments {
                    let target = retreat_point(
                        segment.start,
                        segment.end,
                        f64::from(segment.marker_clearance.max(1)),
                    );
                    draw_arrow(
                        canvas,
                        segment.start.0,
                        segment.start.1,
                        target.0,
                        target.1,
                        color,
                        CONNECTOR_STROKE,
                    );
                }
            }
        }
        ImageArrowType::Next => {
            for i in 0..frame_infos.len() - 1 {
                let src = frame_infos[i].anchor;
                let dst = frame_infos[i + 1].anchor;
                draw_arrow(canvas, src.0, src.1, dst.0, dst.1, color, CONNECTOR_STROKE);
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

fn compute_layout_cols(event_count: usize) -> u32 {
    ((event_count.max(1) as f64).sqrt().ceil() as u32).max(1)
}

fn compute_layout_rows(event_count: usize, cols: u32) -> u32 {
    let cols = cols.max(1) as usize;
    ((event_count + cols - 1) / cols) as u32
}

fn compute_thumbnail_size(event_count: usize, resolution: u32) -> u32 {
    let cols = compute_layout_cols(event_count);
    (resolution / cols).max(24)
}

fn compute_panel_gutter(panel_size: u32) -> u32 {
    (panel_size / PANEL_GUTTER_FRACTION).max(2)
}

fn compute_canvas_dimension(count: u32, panel_size: u32, gutter: u32) -> u32 {
    let count = count.max(1);
    count
        .saturating_mul(panel_size)
        .saturating_add((count + 1).saturating_mul(gutter))
}

fn draw_axes(canvas: &mut RgbImage) {
    let width = i32::try_from(canvas.width()).expect("canvas width must fit i32");
    let height = i32::try_from(canvas.height()).expect("canvas height must fit i32");
    let center_x = width / 2;
    let center_y = height / 2;
    let axis = Rgb([0, 0, 0]);

    draw_line_thick(canvas, 0, center_y, width - 1, center_y, axis, AXIS_STROKE);
    draw_line_thick(canvas, center_x, 0, center_x, height - 1, axis, AXIS_STROKE);
}

fn draw_axes_in_box(canvas: &mut RgbImage, origin_x: i32, origin_y: i32, size: i32, stroke: i32) {
    draw_axes_in_box_with_style(canvas, origin_x, origin_y, size, stroke, Rgb([0, 0, 0]));
}

fn draw_axes_in_box_with_style(
    canvas: &mut RgbImage,
    origin_x: i32,
    origin_y: i32,
    size: i32,
    stroke: i32,
    color: Rgb<u8>,
) {
    let min_x = origin_x;
    let min_y = origin_y;
    let max_x = origin_x + size - 1;
    let max_y = origin_y + size - 1;
    let center_x = (origin_x + max_x) / 2;
    let center_y = (origin_y + max_y) / 2;

    draw_line_thick(canvas, min_x, center_y, max_x, center_y, color, stroke);
    draw_line_thick(canvas, center_x, min_y, center_x, max_y, color, stroke);
}

fn draw_panel_border(canvas: &mut RgbImage, origin_x: i32, origin_y: i32, size: i32) {
    let color = Rgb([0, 0, 0]);
    let max_x = origin_x + size - 1;
    let max_y = origin_y + size - 1;

    draw_line_thick(
        canvas,
        origin_x,
        origin_y,
        max_x,
        origin_y,
        color,
        PANEL_BORDER_STROKE,
    );
    draw_line_thick(
        canvas,
        origin_x,
        max_y,
        max_x,
        max_y,
        color,
        PANEL_BORDER_STROKE,
    );
    draw_line_thick(
        canvas,
        origin_x,
        origin_y,
        origin_x,
        max_y,
        color,
        PANEL_BORDER_STROKE,
    );
    draw_line_thick(
        canvas,
        max_x,
        origin_y,
        max_x,
        max_y,
        color,
        PANEL_BORDER_STROKE,
    );
}

fn draw_panel_diagonal_guides(canvas: &mut RgbImage, origin_x: i32, origin_y: i32, size: i32) {
    let max_x = origin_x + size - 1;
    let max_y = origin_y + size - 1;
    let guide = Rgb(PANEL_DIAGONAL_GUIDE_COLOR);
    draw_line(canvas, max_x, max_y, origin_x, origin_y, guide);
    draw_line(canvas, origin_x, max_y, max_x, origin_y, guide);
}

fn marker_radius(resolution: u32) -> i32 {
    let scaled = (resolution / 64).max(5);
    scaled as i32
}

fn marker_radius_for_size(size: u32) -> i32 {
    let scaled = (size / 20).max(5);
    scaled as i32
}

fn marker_radius_for_shape(base_radius: i32, shape_type: &str) -> i32 {
    match shape_type {
        "triangle" => (base_radius * 3 + 1) / 2,
        "star" => base_radius * 2,
        "circle" => base_radius + 2,
        _ => base_radius + 1,
    }
}

fn fill_rect(canvas: &mut RgbImage, x: i32, y: i32, width: i32, height: i32, color: Rgb<u8>) {
    for dy in 0..height {
        for dx in 0..width {
            set_pixel_if_in_bounds(canvas, x + dx, y + dy, color);
        }
    }
}

fn draw_shape_geometry(
    canvas: &mut RgbImage,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: Rgb<u8>,
    shape_type: &str,
) {
    let outline = Rgb([0, 0, 0]);
    match shape_type {
        "circle" => {
            draw_filled_circle(canvas, center_x, center_y, radius + 1, outline);
            draw_filled_circle(canvas, center_x, center_y, radius, color);
        }
        "triangle" => {
            let points = regular_polygon_points(
                center_x,
                center_y,
                radius,
                3,
                -core::f64::consts::FRAC_PI_2,
            );
            draw_filled_polygon(canvas, &points, color);
            draw_polygon_outline(canvas, &points, outline, SHAPE_OUTLINE_STROKE);
        }
        "square" => {
            let points =
                regular_polygon_points(center_x, center_y, radius, 4, core::f64::consts::FRAC_PI_4);
            draw_filled_polygon(canvas, &points, color);
            draw_polygon_outline(canvas, &points, outline, SHAPE_OUTLINE_STROKE);
        }
        "pentagon" => {
            let points =
                regular_polygon_points(center_x, center_y, radius, 5, -core::f64::consts::PI / 2.0);
            draw_filled_polygon(canvas, &points, color);
            draw_polygon_outline(canvas, &points, outline, SHAPE_OUTLINE_STROKE);
        }
        "hexagon" => {
            let points =
                regular_polygon_points(center_x, center_y, radius, 6, -core::f64::consts::PI / 2.0);
            draw_filled_polygon(canvas, &points, color);
            draw_polygon_outline(canvas, &points, outline, SHAPE_OUTLINE_STROKE);
        }
        "star" => {
            let mut points = Vec::with_capacity(10);
            let outer = f64::from(radius);
            let inner = outer * 0.45;
            let cx = center_x as f64;
            let cy = center_y as f64;
            for i in 0..10 {
                let angle = -core::f64::consts::PI / 2.0 + core::f64::consts::PI / 5.0 * (i as f64);
                let r = if i % 2 == 0 { outer } else { inner };
                let x = (cx + r * angle.cos()).round() as i32;
                let y = (cy + r * angle.sin()).round() as i32;
                points.push((x, y));
            }
            draw_filled_polygon(canvas, &points, color);
            draw_polygon_outline(canvas, &points, outline, SHAPE_OUTLINE_STROKE);
        }
        _ => {
            draw_filled_circle(canvas, center_x, center_y, radius + 1, outline);
            draw_filled_circle(canvas, center_x, center_y, radius, color);
        }
    }
}

fn regular_polygon_points(
    center_x: i32,
    center_y: i32,
    radius: i32,
    sides: u32,
    phase_angle: f64,
) -> Vec<(i32, i32)> {
    let mut points = Vec::with_capacity(sides as usize);
    let r = f64::from(radius);
    let cx = center_x as f64;
    let cy = center_y as f64;
    for i in 0..sides {
        let angle = phase_angle + 2.0 * core::f64::consts::PI * (i as f64) / (sides as f64);
        let x = (cx + r * angle.cos()).round() as i32;
        let y = (cy + r * angle.sin()).round() as i32;
        points.push((x, y));
    }
    points
}

fn draw_filled_polygon(canvas: &mut RgbImage, points: &[(i32, i32)], color: Rgb<u8>) {
    if points.len() < 3 {
        return;
    }

    let mut min_x = points[0].0;
    let mut max_x = points[0].0;
    let mut min_y = points[0].1;
    let mut max_y = points[0].1;

    for &(x, y) in points {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if point_in_polygon(x, y, points) {
                set_pixel_if_in_bounds(canvas, x, y, color);
            }
        }
    }
}

fn point_in_polygon(x: i32, y: i32, points: &[(i32, i32)]) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;

    for i in 0..points.len() {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];

        if (yi > y) != (yj > y) {
            let denom = (yj - yi) as i64;
            if denom != 0 {
                let x_intersect =
                    (xj as i64 - xi as i64) * (y as i64 - yi as i64) / denom + xi as i64;
                if i64::from(x) < x_intersect {
                    inside = !inside;
                }
            }
        }

        j = i;
    }

    inside
}

fn draw_arrow(
    canvas: &mut RgbImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Rgb<u8>,
    stroke: i32,
) {
    let dx = (x1 - x0) as f64;
    let dy = (y1 - y0) as f64;
    let length = (dx * dx + dy * dy).sqrt();
    if length == 0.0 {
        return;
    }

    let unit_x = dx / length;
    let unit_y = dy / length;
    let arrow_size = (f64::from(stroke.max(1)) * ARROW_HEAD_SIZE_MULTIPLIER)
        .max(ARROW_HEAD_MIN_SIZE)
        .min(length * 0.45);
    let theta = core::f64::consts::FRAC_PI_6;
    let cos_theta = theta.cos();
    let sin_theta = theta.sin();
    let tip_x = x1 as f64;
    let tip_y = y1 as f64;
    let shaft_end_x = tip_x - unit_x * (arrow_size * 0.82);
    let shaft_end_y = tip_y - unit_y * (arrow_size * 0.82);

    let left_x = tip_x - arrow_size * (unit_x * cos_theta - unit_y * sin_theta);
    let left_y = tip_y - arrow_size * (unit_x * sin_theta + unit_y * cos_theta);
    let right_x = tip_x - arrow_size * (unit_x * cos_theta + unit_y * sin_theta);
    let right_y = tip_y - arrow_size * (-unit_x * sin_theta + unit_y * cos_theta);

    draw_line_thick(
        canvas,
        x0,
        y0,
        shaft_end_x.round() as i32,
        shaft_end_y.round() as i32,
        color,
        stroke,
    );

    let tip = (x1, y1);
    let left = (left_x.round() as i32, left_y.round() as i32);
    let right = (right_x.round() as i32, right_y.round() as i32);
    draw_filled_polygon(canvas, &[tip, left, right], Rgb([255, 255, 255]));
    draw_line_thick(canvas, tip.0, tip.1, left.0, left.1, color, stroke);
    draw_line_thick(canvas, left.0, left.1, right.0, right.1, color, stroke);
    draw_line_thick(canvas, right.0, right.1, tip.0, tip.1, color, stroke);
}

fn draw_polygon_outline(canvas: &mut RgbImage, points: &[(i32, i32)], color: Rgb<u8>, stroke: i32) {
    if points.len() < 2 {
        return;
    }
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        draw_line_thick(canvas, a.0, a.1, b.0, b.1, color, stroke);
    }
}

fn retreat_point(from: (i32, i32), to: (i32, i32), distance: f64) -> (i32, i32) {
    let dx = f64::from(to.0 - from.0);
    let dy = f64::from(to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= distance || length == 0.0 {
        return to;
    }
    let factor = (length - distance) / length;
    (
        (f64::from(from.0) + dx * factor).round() as i32,
        (f64::from(from.1) + dy * factor).round() as i32,
    )
}

fn draw_line_thick(
    canvas: &mut RgbImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Rgb<u8>,
    thickness: i32,
) {
    let half = thickness.max(1) / 2;
    for dy in -half..=half {
        for dx in -half..=half {
            if dx * dx + dy * dy <= half * half {
                draw_line(canvas, x0 + dx, y0 + dy, x1 + dx, y1 + dy, color);
            }
        }
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

fn color_name_to_rgb(color: &str) -> Rgb<u8> {
    match color {
        "red" => Rgb([220, 20, 60]),
        "green" => Rgb([30, 160, 60]),
        "blue" => Rgb([30, 90, 220]),
        "yellow" => Rgb([230, 190, 40]),
        "magenta" => Rgb([180, 50, 170]),
        "cyan" => Rgb([20, 170, 190]),
        // Backward-compatible aliases for older materializations.
        "purple" => Rgb([180, 50, 170]),
        "orange" => Rgb([230, 120, 20]),
        "white" => Rgb([200, 200, 200]),
        _ => Rgb([60, 60, 60]),
    }
}

fn normalized_to_pixel(x: f64, y: f64, resolution: u32) -> (i32, i32) {
    let max = (resolution.saturating_sub(1)) as f64;
    let px = (((x + 1.0) * 0.5) * max).round().clamp(0.0, max) as i32;
    let py = (((1.0 - (y + 1.0) * 0.5) * max).round()).clamp(0.0, max) as i32;
    (px, py)
}

fn inverse_color(color: Rgb<u8>) -> Rgb<u8> {
    Rgb([
        255u8.wrapping_sub(color[0]),
        255u8.wrapping_sub(color[1]),
        255u8.wrapping_sub(color[2]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_generation::{
        MotionEvent, MotionEventAccounting, SceneGenerationOutput, SceneGenerationParams,
        SceneProjectionMode, SceneShapePath, generate_scene,
    };
    use crate::seed_schedule::SceneSeedSchedule;
    use crate::{NormalizedPoint, ShapeFlowConfig};
    use image::RgbImage;

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config must parse")
    }

    fn make_test_scene(shape_count: usize, event_count: usize) -> SceneGenerationOutput {
        let schedule = SceneSeedSchedule::derive(1, 0);
        let shape_paths = (0..shape_count)
            .map(|index| SceneShapePath {
                shape_index: index,
                trajectory_points: vec![NormalizedPoint::new(0.0, 0.0).expect("point must build")],
                soft_memberships: None,
            })
            .collect::<Vec<_>>();

        let mut motion_events = Vec::with_capacity(event_count);
        for event_index in 0..event_count {
            let shape_index = event_index % shape_count;
            motion_events.push(MotionEvent {
                global_event_index: u32::try_from(event_index).expect("event index fits"),
                time_slot: u32::try_from(event_index).expect("time slot fits"),
                shape_index,
                shape_event_index: u16::try_from(event_index).expect("shape event index fits"),
                start_point: NormalizedPoint::new(0.0, 0.0).expect("point must build"),
                end_point: NormalizedPoint::new(0.2, 0.1).expect("point must build"),
                duration_frames: 12,
                easing: crate::config::EasingFamily::Linear,
            });
        }

        SceneGenerationOutput {
            scene_index: 0,
            schedule,
            shape_identity_assignment: crate::config::ShapeIdentityAssignment::IndexLocked,
            shape_paths,
            motion_events,
            accounting: MotionEventAccounting {
                expected_total: u32::try_from(event_count).expect("expected total fits"),
                expected_slots: u32::try_from(event_count).expect("expected slots fit"),
                generated_total: u32::try_from(event_count).expect("generated total fits"),
                expected_per_shape: vec![
                    u16::try_from(event_count).expect("expected per shape fits"),
                ],
                generated_per_shape: vec![
                    u16::try_from(event_count).expect("generated per shape fits"),
                ],
            },
        }
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
    fn rendered_image_png_with_scene_config_is_deterministic_and_dynamic_size() {
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
        let panel_count = build_time_slot_panels(&scene, config.scene.n_motion_slots).len();
        let cols = compute_layout_cols(panel_count);
        let rows = compute_layout_rows(panel_count, cols);
        let panel = compute_thumbnail_size(panel_count, config.scene.resolution);
        let gutter = compute_panel_gutter(panel);
        assert_eq!(
            decoded.width(),
            compute_canvas_dimension(cols, panel, gutter)
        );
        assert_eq!(
            decoded.height(),
            compute_canvas_dimension(rows, panel, gutter)
        );
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
    fn scene_image_rendering_uses_shape_geometry() {
        let white = Rgb([255, 255, 255]).0;
        let mut canvas = RgbImage::from_pixel(64, 64, Rgb([255, 255, 255]));
        draw_shape_geometry(&mut canvas, 20, 20, 8, Rgb([220, 20, 60]), "circle");
        let circle_count = canvas.pixels().filter(|pixel| pixel.0 != white).count();

        let mut canvas = RgbImage::from_pixel(64, 64, Rgb([255, 255, 255]));
        draw_shape_geometry(&mut canvas, 20, 20, 8, Rgb([220, 20, 60]), "triangle");
        let triangle_count = canvas.pixels().filter(|pixel| pixel.0 != white).count();

        let mut canvas = RgbImage::from_pixel(64, 64, Rgb([255, 255, 255]));
        draw_shape_geometry(&mut canvas, 20, 20, 8, Rgb([220, 20, 60]), "square");
        let square_count = canvas.pixels().filter(|pixel| pixel.0 != white).count();

        assert_ne!(circle_count, triangle_count);
        assert_ne!(circle_count, square_count);
    }

    #[test]
    fn panel_layout_is_deterministic_not_row_major() {
        let config = bootstrap_config();
        let scene = make_test_scene(2, 6);
        let panel_count = build_time_slot_panels(&scene, config.scene.n_motion_slots).len();

        let cols = compute_layout_cols(panel_count);
        let panel = compute_thumbnail_size(panel_count, config.scene.resolution);
        let gutter = compute_panel_gutter(panel);
        let placements_first =
            build_thumbnail_placements(panel_count, &scene, &config.scene, panel, cols, gutter);
        let placements_second =
            build_thumbnail_placements(panel_count, &scene, &config.scene, panel, cols, gutter);
        assert_eq!(placements_first, placements_second);

        let expected: Vec<ThumbnailPlacement> = (0..panel_count)
            .map(|panel_index| {
                let col = panel_index as u32 % cols;
                let row = panel_index as u32 / cols;
                ThumbnailPlacement {
                    origin_x: i32::try_from(col * (panel + gutter) + gutter)
                        .expect("x should fit i32"),
                    origin_y: i32::try_from(row * (panel + gutter) + gutter)
                        .expect("y should fit i32"),
                }
            })
            .collect();

        assert_ne!(placements_first, expected);
    }

    #[test]
    fn dynamic_canvas_size_depends_on_panel_count() {
        let mut cfg_small = bootstrap_config();
        let mut cfg_many = bootstrap_config();
        cfg_small.scene.n_motion_slots = 2;
        cfg_many.scene.n_motion_slots = 9;

        let scene_small = make_test_scene(1, 1);
        let scene_many = make_test_scene(2, 7);

        let img_small = render_scene_image_png_with_scene_config(&scene_small, &cfg_small.scene)
            .expect("image generation should succeed");
        let img_many = render_scene_image_png_with_scene_config(&scene_many, &cfg_many.scene)
            .expect("image generation should succeed");

        let decoded_small =
            image::load_from_memory(&img_small).expect("generated image should decode");
        let decoded_many =
            image::load_from_memory(&img_many).expect("generated image should decode");
        assert_ne!(decoded_small.width(), decoded_many.width());
        assert_ne!(decoded_small.height(), decoded_many.height());
    }

    #[test]
    fn scene_image_anchor_uses_seeded_panel_corners() {
        let config = bootstrap_config();
        let scene = make_test_scene(2, 2);
        let slot_panels = build_time_slot_panels(&scene, config.scene.n_motion_slots);

        let panel_count = slot_panels.len();
        let cols = compute_layout_cols(panel_count);
        let panel = compute_thumbnail_size(panel_count, config.scene.resolution);
        let gutter = compute_panel_gutter(panel);

        let placements =
            build_thumbnail_placements(panel_count, &scene, &config.scene, panel, cols, gutter);
        let placement = placements[0];
        let inset = (panel as f64 * 0.10).round() as i32;

        let mut first = RgbImage::from_pixel(128, 128, Rgb([255, 255, 255]));
        let shape_count = scene.shape_paths.len();
        let mut shape_styles = Vec::with_capacity(shape_count);
        for shape_index in 0..shape_count {
            let identity =
                shape_identity_for_scene(&scene, shape_index).expect("shape identity should build");
            shape_styles.push((
                color_name_to_rgb(identity.color.as_str()),
                identity.shape_type,
            ));
        }
        let first_info = render_time_slot_frame(
            &mut first,
            &scene,
            &slot_panels[0],
            &shape_styles,
            shape_count,
            placement,
            panel,
            panel / PANEL_PADDING_FRACTION,
        )
        .expect("slot frame render should succeed");
        let expected_corners = [
            (placement.origin_x + inset, placement.origin_y + inset),
            (
                placement.origin_x + i32::try_from(panel).expect("panel size fit") - inset - 1,
                placement.origin_y + inset,
            ),
            (
                placement.origin_x + inset,
                placement.origin_y + i32::try_from(panel).expect("panel size fit") - inset - 1,
            ),
            (
                placement.origin_x + i32::try_from(panel).expect("panel size fit") - inset - 1,
                placement.origin_y + i32::try_from(panel).expect("panel size fit") - inset - 1,
            ),
        ];
        assert!(expected_corners.contains(&first_info.anchor));

        let second_info = render_time_slot_frame(
            &mut RgbImage::from_pixel(128, 128, Rgb([255, 255, 255])),
            &scene,
            &slot_panels[0],
            &shape_styles,
            shape_count,
            placement,
            panel,
            panel / PANEL_PADDING_FRACTION,
        )
        .expect("slot frame render should succeed");

        assert_eq!(first_info.anchor, second_info.anchor);
    }

    #[test]
    fn scene_image_anchor_varies_across_panels_in_same_scene() {
        let config = bootstrap_config();
        let scene = make_test_scene(2, 3);
        let slot_panels = build_time_slot_panels(&scene, config.scene.n_motion_slots);
        let panel_count = slot_panels.len();
        let cols = compute_layout_cols(panel_count);
        let panel = compute_thumbnail_size(panel_count, config.scene.resolution);
        let gutter = compute_panel_gutter(panel);

        let placements =
            build_thumbnail_placements(panel_count, &scene, &config.scene, panel, cols, gutter);
        let shape_count = scene.shape_paths.len();
        let padding = panel / PANEL_PADDING_FRACTION;
        let mut anchors = Vec::with_capacity(slot_panels.len());

        let mut shape_styles = Vec::with_capacity(shape_count);
        for shape_index in 0..shape_count {
            let identity =
                shape_identity_for_scene(&scene, shape_index).expect("shape identity should build");
            shape_styles.push((
                color_name_to_rgb(identity.color.as_str()),
                identity.shape_type,
            ));
        }

        for (panel_index, panel_ref) in slot_panels.iter().enumerate() {
            let mut canvas = RgbImage::from_pixel(128, 128, Rgb([255, 255, 255]));
            let placement = placements[panel_index];
            let frame_info = render_time_slot_frame(
                &mut canvas,
                &scene,
                panel_ref,
                &shape_styles,
                shape_count,
                placement,
                panel,
                padding,
            )
            .expect("slot frame render should succeed");
            anchors.push(frame_info.anchor);
        }

        assert_ne!(anchors[0], anchors[1]);
        assert!(anchors.windows(2).any(|window| window[0] != window[1]));
    }

    #[test]
    fn comic_panel_axis_overlay_uses_red_centerline() {
        let mut config = bootstrap_config();
        config.scene.image_frame_scatter = false;
        let mut scene = make_test_scene(1, 1);
        scene.motion_events[0].end_point =
            NormalizedPoint::new(0.2, 0.2).expect("point must build");

        let shape_count = scene.shape_paths.len();
        let mut shape_styles = Vec::with_capacity(shape_count);
        for shape_index in 0..shape_count {
            let identity =
                shape_identity_for_scene(&scene, shape_index).expect("shape identity should build");
            shape_styles.push((
                color_name_to_rgb(identity.color.as_str()),
                identity.shape_type,
            ));
        }

        let slot_panels = build_time_slot_panels(&scene, config.scene.n_motion_slots);
        let panel_count = slot_panels.len();
        let cols = compute_layout_cols(panel_count);
        let rows = compute_layout_rows(panel_count, cols);
        let panel = compute_thumbnail_size(panel_count, config.scene.resolution);
        let gutter = compute_panel_gutter(panel);
        let padding = (panel / PANEL_PADDING_FRACTION).max(1);
        let canvas_w = compute_canvas_dimension(cols, panel, gutter);
        let canvas_h = compute_canvas_dimension(rows, panel, gutter);
        let mut canvas = RgbImage::from_pixel(canvas_w, canvas_h, Rgb([255, 255, 255]));
        let placements =
            build_thumbnail_placements(panel_count, &scene, &config.scene, panel, cols, gutter);

        let placement = placements[0];
        render_time_slot_frame(
            &mut canvas,
            &scene,
            &slot_panels[0],
            &shape_styles,
            shape_count,
            placement,
            panel,
            padding,
        )
        .expect("slot frame render should succeed");

        let padding_i32 = i32::try_from(padding).expect("padding must fit i32");
        let inner_size = i32::try_from(i64::from(panel) - 2 * i64::from(padding))
            .expect("inner size must fit i32");
        let inner_origin_x = placement.origin_x + padding_i32;
        let inner_origin_y = placement.origin_y + padding_i32;
        let axis_x = (inner_origin_x + (inner_origin_x + inner_size - 1)) / 2;
        let axis_y = (inner_origin_y + (inner_origin_y + inner_size - 1)) / 2;

        assert_eq!(
            canvas
                .get_pixel(
                    axis_x.try_into().expect("x should fit u32"),
                    axis_y.try_into().expect("y should fit u32")
                )
                .0,
            [255, 0, 0]
        );
    }

    #[test]
    fn shape_markers_include_inverse_color_center_dot() {
        let mut config = bootstrap_config();
        config.scene.image_frame_scatter = false;
        let mut scene = make_test_scene(1, 1);
        scene.motion_events[0].end_point =
            NormalizedPoint::new(0.75, -0.45).expect("point must build");

        let shape_count = scene.shape_paths.len();
        let mut shape_styles = Vec::with_capacity(shape_count);
        for shape_index in 0..shape_count {
            let identity =
                shape_identity_for_scene(&scene, shape_index).expect("shape identity should build");
            shape_styles.push((
                color_name_to_rgb(identity.color.as_str()),
                identity.shape_type,
            ));
        }

        let slot_panels = build_time_slot_panels(&scene, config.scene.n_motion_slots);
        let panel_count = slot_panels.len();
        let cols = compute_layout_cols(panel_count);
        let rows = compute_layout_rows(panel_count, cols);
        let panel = compute_thumbnail_size(panel_count, config.scene.resolution);
        let gutter = compute_panel_gutter(panel);
        let padding = (panel / PANEL_PADDING_FRACTION).max(1);
        let canvas_w = compute_canvas_dimension(cols, panel, gutter);
        let canvas_h = compute_canvas_dimension(rows, panel, gutter);
        let mut canvas = RgbImage::from_pixel(canvas_w, canvas_h, Rgb([255, 255, 255]));
        let placements =
            build_thumbnail_placements(panel_count, &scene, &config.scene, panel, cols, gutter);

        let placement = placements[0];
        render_time_slot_frame(
            &mut canvas,
            &scene,
            &slot_panels[0],
            &shape_styles,
            shape_count,
            placement,
            panel,
            padding,
        )
        .expect("slot frame render should succeed");

        let padding_i32 = i32::try_from(padding).expect("padding must fit i32");
        let inner_u32 = u32::try_from(i64::from(panel) - 2 * i64::from(padding))
            .expect("inner size must fit u32");
        let end_local = normalized_to_pixel(0.75, -0.45, inner_u32);
        let marker_center = (
            placement.origin_x + padding_i32 + end_local.0,
            placement.origin_y + padding_i32 + end_local.1,
        );

        let expected_color = shape_styles[0].0;
        let expected_dot = inverse_color(expected_color);
        let center_pixel = canvas
            .get_pixel(marker_center.0 as u32, marker_center.1 as u32)
            .0;
        assert_eq!(center_pixel, expected_dot.0);
        assert_ne!(center_pixel, expected_color.0);
    }

    #[test]
    fn simultaneous_events_share_single_comic_panel() {
        let config = bootstrap_config();
        let mut scene = make_test_scene(2, 2);
        for event in &mut scene.motion_events {
            event.time_slot = 0;
        }
        let panels = build_time_slot_panels(&scene, config.scene.n_motion_slots);
        assert_eq!(
            panels.len(),
            usize::try_from(config.scene.n_motion_slots).expect("slots fit")
        );
        assert_eq!(panels[0].event_indices.len(), 2);
        assert!(
            panels
                .iter()
                .skip(1)
                .all(|panel| panel.event_indices.is_empty())
        );

        let image = render_scene_image_png_with_scene_config(&scene, &config.scene)
            .expect("image generation should succeed");
        let decoded = image::load_from_memory(&image).expect("generated image should decode");
        let panel_count = usize::try_from(config.scene.n_motion_slots).expect("slots fit");
        let cols = compute_layout_cols(panel_count);
        let rows = compute_layout_rows(panel_count, cols);
        let panel = compute_thumbnail_size(panel_count, config.scene.resolution);
        let gutter = compute_panel_gutter(panel);
        assert_eq!(
            decoded.width(),
            compute_canvas_dimension(cols, panel, gutter)
        );
        assert_eq!(
            decoded.height(),
            compute_canvas_dimension(rows, panel, gutter)
        );
    }

    #[test]
    fn current_arrow_backoff_preserves_endpoint_marker_pixels() {
        let preserved = retreat_point((10, 10), (50, 10), 8.0);
        assert!(preserved.0 < 50);
        assert_eq!(preserved.1, 10);
    }

    #[test]
    fn arrow_head_is_white_filled_triangle() {
        let mut canvas = RgbImage::from_pixel(120, 40, Rgb([255, 255, 255]));
        draw_arrow(
            &mut canvas,
            10,
            20,
            100,
            20,
            Rgb([0, 0, 0]),
            CONNECTOR_STROKE,
        );

        let shaft_pixel = canvas.get_pixel(60, 20).0;
        assert_eq!(shaft_pixel, [0, 0, 0]);

        let head_interior = canvas.get_pixel(92, 20).0;
        assert_eq!(head_interior, [255, 255, 255]);

        let head_outline = canvas.get_pixel(100, 20).0;
        assert_eq!(head_outline, [0, 0, 0]);
    }

    #[test]
    fn render_fails_on_invalid_shape_index() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_identity_assignment: crate::config::ShapeIdentityAssignment::IndexLocked,
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
                expected_slots: 1,
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
