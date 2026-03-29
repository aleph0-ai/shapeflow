use crate::config::EasingFamily;
use crate::scene_generation::SceneGenerationOutput;
use std::collections::BTreeMap;
use std::fmt::Write as _;

const SHAPE_TYPE_PALETTE: [&str; 5] = ["circle", "triangle", "square", "pentagon", "star"];
const COLOR_PALETTE: [&str; 8] = [
    "red", "blue", "green", "yellow", "orange", "purple", "white", "cyan",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeIdentity {
    pub shape_index: usize,
    pub shape_id: String,
    pub shape_type: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabularMotionRow {
    pub scene_id: String,
    pub event_index: u32,
    pub shape_id: String,
    pub shape_type: String,
    pub color: String,
    pub start_x: f64,
    pub start_y: f64,
    pub end_x: f64,
    pub end_y: f64,
    pub duration_frames: u32,
    pub easing: String,
    pub simultaneous_shapes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TabularEncodingError {
    #[error("shape index {shape_index} out of bounds for scene with {shape_count} shapes")]
    ShapeIndexOutOfBounds {
        shape_index: usize,
        shape_count: usize,
    },
    #[error("shape index {shape_index} exceeds available shape-type palette length {palette_len}")]
    ShapeTypePaletteExhausted {
        shape_index: usize,
        palette_len: usize,
    },
}

pub fn canonical_scene_id(scene_index: u64) -> String {
    format!("{scene_index:032x}")
}

pub fn shape_identity_for_index(shape_index: usize) -> Result<ShapeIdentity, TabularEncodingError> {
    let shape_type = SHAPE_TYPE_PALETTE.get(shape_index).ok_or(
        TabularEncodingError::ShapeTypePaletteExhausted {
            shape_index,
            palette_len: SHAPE_TYPE_PALETTE.len(),
        },
    )?;
    let color = COLOR_PALETTE[shape_index % COLOR_PALETTE.len()];
    Ok(ShapeIdentity {
        shape_index,
        shape_id: format!("{shape_type}_{color}"),
        shape_type: (*shape_type).to_string(),
        color: color.to_string(),
    })
}

pub fn generate_tabular_motion_rows(
    scene: &SceneGenerationOutput,
) -> Result<Vec<TabularMotionRow>, TabularEncodingError> {
    let shape_count = scene.shape_paths.len();
    let mut shape_identities = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        shape_identities.push(shape_identity_for_index(shape_index)?);
    }

    let mut time_slot_shapes: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for event in &scene.motion_events {
        if event.shape_index >= shape_count {
            return Err(TabularEncodingError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        time_slot_shapes
            .entry(event.time_slot)
            .or_default()
            .push(event.shape_index);
    }
    for shape_indices in time_slot_shapes.values_mut() {
        shape_indices.sort_unstable();
        shape_indices.dedup();
    }

    let scene_id = canonical_scene_id(scene.scene_index);
    let mut rows = Vec::with_capacity(scene.motion_events.len());
    for event in &scene.motion_events {
        let identity = &shape_identities[event.shape_index];
        let peers = time_slot_shapes
            .get(&event.time_slot)
            .expect("time-slot index must exist for all listed events")
            .iter()
            .copied()
            .filter(|shape_index| *shape_index != event.shape_index)
            .map(|shape_index| shape_identities[shape_index].shape_id.clone())
            .collect::<Vec<_>>();
        rows.push(TabularMotionRow {
            scene_id: scene_id.clone(),
            event_index: event.global_event_index,
            shape_id: identity.shape_id.clone(),
            shape_type: identity.shape_type.clone(),
            color: identity.color.clone(),
            start_x: event.start_point.x,
            start_y: event.start_point.y,
            end_x: event.end_point.x,
            end_y: event.end_point.y,
            duration_frames: u32::from(event.duration_frames),
            easing: easing_family_name(event.easing).to_string(),
            simultaneous_shapes: peers,
        });
    }

    Ok(rows)
}

pub fn serialize_tabular_motion_rows_csv(rows: &[TabularMotionRow]) -> String {
    let mut csv = String::new();
    csv.push_str("scene_id,event_index,shape_id,shape_type,color,start_x,start_y,end_x,end_y,duration_frames,easing,simultaneous_shapes\n");
    for row in rows {
        let simultaneous_shapes = row.simultaneous_shapes.join("|");
        writeln!(
            csv,
            "{},{},{},{},{},{:.17},{:.17},{:.17},{:.17},{},{},{}",
            row.scene_id,
            row.event_index,
            row.shape_id,
            row.shape_type,
            row.color,
            row.start_x,
            row.start_y,
            row.end_x,
            row.end_y,
            row.duration_frames,
            row.easing,
            simultaneous_shapes
        )
        .expect("writing to String must succeed");
    }
    csv
}

fn easing_family_name(easing: EasingFamily) -> &'static str {
    match easing {
        EasingFamily::Linear => "linear",
        EasingFamily::EaseIn => "ease_in",
        EasingFamily::EaseOut => "ease_out",
        EasingFamily::EaseInOut => "ease_in_out",
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
    use crate::{NormalizedPoint, ShapeFlowConfig};

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config must parse")
    }

    #[test]
    fn generated_rows_are_deterministic() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 1,
            samples_per_event: 12,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let first = generate_tabular_motion_rows(&scene).expect("tabular generation should work");
        let second = generate_tabular_motion_rows(&scene).expect("tabular generation should work");
        assert_eq!(first, second);
        assert_eq!(first.len(), scene.motion_events.len());
    }

    #[test]
    fn simultaneous_shapes_are_emitted_for_shared_time_slots() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 0,
            samples_per_event: 8,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let rows = generate_tabular_motion_rows(&scene).expect("tabular generation should work");

        assert!(
            !rows.is_empty(),
            "bootstrap fixture should generate at least one row"
        );
        assert_eq!(rows[0].shape_id, "circle_red");
        assert_eq!(rows[0].simultaneous_shapes, vec!["triangle_blue"]);
    }

    #[test]
    fn csv_serialization_writes_header_and_rows() {
        let rows = vec![TabularMotionRow {
            scene_id: "00000000000000000000000000000000".to_string(),
            event_index: 0,
            shape_id: "circle_red".to_string(),
            shape_type: "circle".to_string(),
            color: "red".to_string(),
            start_x: -0.25,
            start_y: 0.5,
            end_x: 0.25,
            end_y: -0.5,
            duration_frames: 24,
            easing: "ease_in_out".to_string(),
            simultaneous_shapes: vec!["triangle_blue".to_string()],
        }];
        let csv = serialize_tabular_motion_rows_csv(&rows);

        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "scene_id,event_index,shape_id,shape_type,color,start_x,start_y,end_x,end_y,duration_frames,easing,simultaneous_shapes"
        );
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("circle_red"));
        assert!(lines[1].contains("triangle_blue"));
    }

    #[test]
    fn duplicate_slot_shape_indices_emit_deduplicated_peer_lists() {
        let origin = NormalizedPoint::new(0.0, 0.0).expect("point must build");
        let shape_paths = vec![
            SceneShapePath {
                shape_index: 0,
                trajectory_points: vec![origin],
                soft_memberships: None,
            },
            SceneShapePath {
                shape_index: 1,
                trajectory_points: vec![origin],
                soft_memberships: None,
            },
            SceneShapePath {
                shape_index: 2,
                trajectory_points: vec![origin],
                soft_memberships: None,
            },
        ];
        let motion_events = vec![
            MotionEvent {
                global_event_index: 0,
                time_slot: 7,
                shape_index: 0,
                shape_event_index: 0,
                start_point: origin,
                end_point: origin,
                duration_frames: 24,
                easing: EasingFamily::Linear,
            },
            MotionEvent {
                global_event_index: 1,
                time_slot: 7,
                shape_index: 1,
                shape_event_index: 0,
                start_point: origin,
                end_point: origin,
                duration_frames: 24,
                easing: EasingFamily::Linear,
            },
            MotionEvent {
                global_event_index: 2,
                time_slot: 7,
                shape_index: 1,
                shape_event_index: 1,
                start_point: origin,
                end_point: origin,
                duration_frames: 24,
                easing: EasingFamily::Linear,
            },
            MotionEvent {
                global_event_index: 3,
                time_slot: 7,
                shape_index: 2,
                shape_event_index: 0,
                start_point: origin,
                end_point: origin,
                duration_frames: 24,
                easing: EasingFamily::Linear,
            },
        ];
        let scene = SceneGenerationOutput {
            scene_index: 9,
            schedule: SceneSeedSchedule::derive(123, 9),
            shape_paths,
            motion_events,
            accounting: MotionEventAccounting {
                expected_total: 4,
                generated_total: 4,
                expected_per_shape: vec![1, 2, 1],
                generated_per_shape: vec![1, 2, 1],
            },
        };

        let rows = generate_tabular_motion_rows(&scene).expect("tabular generation should work");
        assert_eq!(rows.len(), 4);

        let row0 = &rows[0];
        assert_eq!(row0.shape_id, "circle_red");
        assert_eq!(
            row0.simultaneous_shapes,
            vec!["triangle_blue".to_string(), "square_green".to_string()]
        );

        for row in &rows {
            let self_occurrences = row
                .simultaneous_shapes
                .iter()
                .filter(|peer| **peer == row.shape_id)
                .count();
            assert_eq!(
                self_occurrences, 0,
                "row peers must exclude the event shape itself"
            );

            let unique_peers = row
                .simultaneous_shapes
                .iter()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                unique_peers.len(),
                row.simultaneous_shapes.len(),
                "peer list must be deduplicated even when slot events repeat a shape index"
            );
        }
    }

    #[test]
    fn generation_fails_on_invalid_shape_index() {
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
                easing: EasingFamily::Linear,
            }],
            accounting: MotionEventAccounting {
                expected_total: 1,
                generated_total: 1,
                expected_per_shape: vec![1],
                generated_per_shape: vec![1],
            },
        };

        let err = generate_tabular_motion_rows(&scene).expect_err("invalid shape index must fail");
        assert!(matches!(
            err,
            TabularEncodingError::ShapeIndexOutOfBounds {
                shape_index: 1,
                shape_count: 1
            }
        ));
    }
}
