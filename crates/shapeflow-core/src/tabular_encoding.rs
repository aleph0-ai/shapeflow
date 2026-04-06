use crate::config::ShapeIdentityAssignment;
use crate::scene_generation::SceneGenerationOutput;
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;
use std::fmt::Write as _;

pub const SHAPE_TYPE_PALETTE: [&str; 6] = [
    "circle", "star", "triangle", "square", "pentagon", "hexagon",
];

pub const COLOR_PALETTE: [&str; 6] = ["red", "green", "blue", "yellow", "magenta", "cyan"];

const SHAPE_CODE_PALETTE: [u8; 6] = [1, 2, 3, 4, 5, 6];
const COLOR_BIT_CODE_PALETTE: [u8; 6] = [0b100, 0b010, 0b001, 0b110, 0b101, 0b011];

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
    pub shape_type: String,
    pub color: String,
    pub start_x: f64,
    pub start_y: f64,
    pub end_x: f64,
    pub end_y: f64,
    pub simultaneous_event_ids: Vec<u32>,
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
    #[error("color index {color_index} exceeds available color palette length {palette_len}")]
    ColorPaletteExhausted {
        color_index: usize,
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

pub fn shape_code_for_shape_type_index(
    shape_type_index: usize,
) -> Result<u8, TabularEncodingError> {
    SHAPE_CODE_PALETTE.get(shape_type_index).copied().ok_or(
        TabularEncodingError::ShapeTypePaletteExhausted {
            shape_index: shape_type_index,
            palette_len: SHAPE_CODE_PALETTE.len(),
        },
    )
}

pub fn color_bits_for_color_index(color_index: usize) -> Result<u8, TabularEncodingError> {
    COLOR_BIT_CODE_PALETTE.get(color_index).copied().ok_or(
        TabularEncodingError::ColorPaletteExhausted {
            color_index,
            palette_len: COLOR_BIT_CODE_PALETTE.len(),
        },
    )
}

pub fn canonical_class_value_for_shape_type_and_color(
    shape_type_index: usize,
    color_index: usize,
) -> Result<u8, TabularEncodingError> {
    let shape_code = shape_code_for_shape_type_index(shape_type_index)?;
    let color_bits = color_bits_for_color_index(color_index)?;
    Ok((shape_code << 3) | color_bits)
}

pub fn canonical_class_rank_for_shape_type_and_color(
    shape_type_index: usize,
    color_index: usize,
) -> Result<u8, TabularEncodingError> {
    let class_value =
        canonical_class_value_for_shape_type_and_color(shape_type_index, color_index)?;
    let mut rank = 0u8;
    for candidate_shape in 0..SHAPE_TYPE_PALETTE.len() {
        for candidate_color in 0..COLOR_PALETTE.len() {
            let candidate_value =
                canonical_class_value_for_shape_type_and_color(candidate_shape, candidate_color)?;
            if candidate_value < class_value {
                rank = rank.saturating_add(1);
            }
        }
    }
    Ok(rank)
}

pub fn canonical_class_value_for_scene_seed(
    scene_layout_seed: u64,
    shape_identity_assignment: ShapeIdentityAssignment,
    shape_index: usize,
) -> Result<u8, TabularEncodingError> {
    let (shape_type_index, color_index) = shape_type_and_color_indices_for_scene_seed(
        scene_layout_seed,
        shape_identity_assignment,
        shape_index,
    )?;
    canonical_class_value_for_shape_type_and_color(shape_type_index, color_index)
}

pub fn canonical_class_rank_for_scene_seed(
    scene_layout_seed: u64,
    shape_identity_assignment: ShapeIdentityAssignment,
    shape_index: usize,
) -> Result<u8, TabularEncodingError> {
    let (shape_type_index, color_index) = shape_type_and_color_indices_for_scene_seed(
        scene_layout_seed,
        shape_identity_assignment,
        shape_index,
    )?;
    canonical_class_rank_for_shape_type_and_color(shape_type_index, color_index)
}

pub fn canonical_class_count() -> usize {
    SHAPE_TYPE_PALETTE.len() * COLOR_PALETTE.len()
}

pub fn shape_identity_for_scene(
    scene: &SceneGenerationOutput,
    shape_index: usize,
) -> Result<ShapeIdentity, TabularEncodingError> {
    shape_identity_for_scene_seed(
        scene.schedule.scene_layout,
        scene.shape_identity_assignment,
        shape_index,
    )
}

pub fn shape_identity_for_scene_seed(
    scene_layout_seed: u64,
    shape_identity_assignment: ShapeIdentityAssignment,
    shape_index: usize,
) -> Result<ShapeIdentity, TabularEncodingError> {
    let (shape_type_index, color_index) = shape_type_and_color_indices_for_scene_seed(
        scene_layout_seed,
        shape_identity_assignment,
        shape_index,
    )?;
    let shape_type: &str = SHAPE_TYPE_PALETTE.get(shape_type_index).ok_or(
        TabularEncodingError::ShapeTypePaletteExhausted {
            shape_index,
            palette_len: SHAPE_TYPE_PALETTE.len(),
        },
    )?;
    let color: &str = COLOR_PALETTE[color_index];
    Ok(ShapeIdentity {
        shape_index,
        shape_id: format!("{shape_type}_{color}"),
        shape_type: (*shape_type).to_string(),
        color: color.to_string(),
    })
}

pub fn color_index_for_scene_seed(scene_layout_seed: u64, shape_index: usize) -> usize {
    let mut indices = (0..COLOR_PALETTE.len()).collect::<Vec<_>>();
    let mut rng = ChaCha8Rng::seed_from_u64(scene_layout_seed ^ 0x9E37_79B9_7F4A_7C15);
    for i in (1..indices.len()).rev() {
        let upper = u64::try_from(i + 1).expect("color shuffle upper bound should fit u64");
        let j =
            usize::try_from(rng.next_u64() % upper).expect("color shuffle index should fit usize");
        indices.swap(i, j);
    }
    indices[shape_index % indices.len()]
}

fn shuffled_shape_type_color_pairs(scene_layout_seed: u64) -> Vec<(usize, usize)> {
    let mut pairs = Vec::with_capacity(SHAPE_TYPE_PALETTE.len() * COLOR_PALETTE.len());
    for shape_type_index in 0..SHAPE_TYPE_PALETTE.len() {
        for color_index in 0..COLOR_PALETTE.len() {
            pairs.push((shape_type_index, color_index));
        }
    }

    let mut rng = ChaCha8Rng::seed_from_u64(scene_layout_seed ^ 0x9E37_79B9_7F4A_7C15);
    for i in (1..pairs.len()).rev() {
        let upper =
            u64::try_from(i + 1).expect("shape-color pair shuffle upper bound should fit u64");
        let j = usize::try_from(rng.next_u64() % upper)
            .expect("shape-color pair shuffle index should fit usize");
        pairs.swap(i, j);
    }

    pairs
}

pub fn shape_type_and_color_indices_for_scene_seed(
    scene_layout_seed: u64,
    shape_identity_assignment: ShapeIdentityAssignment,
    shape_index: usize,
) -> Result<(usize, usize), TabularEncodingError> {
    match shape_identity_assignment {
        ShapeIdentityAssignment::IndexLocked => {
            let color_index = color_index_for_scene_seed(scene_layout_seed, shape_index);
            Ok((shape_index, color_index))
        }
        ShapeIdentityAssignment::PairUniqueRandom => {
            let pairs = shuffled_shape_type_color_pairs(scene_layout_seed);
            pairs
                .get(shape_index)
                .copied()
                .ok_or(TabularEncodingError::ShapeTypePaletteExhausted {
                    shape_index,
                    palette_len: pairs.len(),
                })
        }
    }
}

pub fn generate_tabular_motion_rows(
    scene: &SceneGenerationOutput,
) -> Result<Vec<TabularMotionRow>, TabularEncodingError> {
    let shape_count = scene.shape_paths.len();
    let mut shape_identities = Vec::with_capacity(shape_count);
    for shape_index in 0..shape_count {
        shape_identities.push(shape_identity_for_scene(scene, shape_index)?);
    }

    let mut time_slot_events: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for event in &scene.motion_events {
        if event.shape_index >= shape_count {
            return Err(TabularEncodingError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        time_slot_events
            .entry(event.time_slot)
            .or_default()
            .push(event.global_event_index);
    }
    for event_indices in time_slot_events.values_mut() {
        event_indices.sort_unstable();
        event_indices.dedup();
    }

    let scene_id = canonical_scene_id(scene.scene_index);
    let mut rows = Vec::with_capacity(scene.motion_events.len());
    for event in &scene.motion_events {
        let identity = &shape_identities[event.shape_index];
        let peers = time_slot_events
            .get(&event.time_slot)
            .expect("time-slot index must exist for all listed events")
            .iter()
            .copied()
            .filter(|event_id| *event_id != event.global_event_index)
            .collect::<Vec<_>>();
        rows.push(TabularMotionRow {
            scene_id: scene_id.clone(),
            event_index: event.global_event_index,
            shape_type: identity.shape_type.clone(),
            color: identity.color.clone(),
            start_x: event.start_point.x,
            start_y: event.start_point.y,
            end_x: event.end_point.x,
            end_y: event.end_point.y,
            simultaneous_event_ids: peers,
        });
    }

    Ok(rows)
}

/// Round to 3 decimal places, away from zero.
fn round_away(v: f64) -> f64 {
    let factor = 1000.0;
    let scaled = v * factor;
    if v >= 0.0 {
        scaled.ceil() / factor
    } else {
        scaled.floor() / factor
    }
}

pub fn serialize_tabular_motion_rows_csv(rows: &[TabularMotionRow]) -> String {
    serialize_tabular_csv_inner(rows, true)
}

pub fn serialize_tabular_motion_rows_csv_display(rows: &[TabularMotionRow]) -> String {
    serialize_tabular_csv_inner(rows, false)
}

fn serialize_tabular_csv_inner(rows: &[TabularMotionRow], include_scene_id: bool) -> String {
    let mut csv = String::new();
    if include_scene_id {
        csv.push_str("scene_id,");
    }
    csv.push_str("event_index,shape_type,color,start_x,start_y,end_x,end_y,simultaneous_events\n");
    for row in rows {
        let mut sorted_event_ids = row.simultaneous_event_ids.clone();
        sorted_event_ids.sort_unstable();
        let simultaneous_events = sorted_event_ids
            .into_iter()
            .map(|event_id| event_id.to_string())
            .collect::<Vec<_>>()
            .join("|");
        if include_scene_id {
            write!(csv, "{},", row.scene_id).expect("writing to String must succeed");
        }
        writeln!(
            csv,
            "{},{},{},{:.3},{:.3},{:.3},{:.3},{}",
            row.event_index,
            row.shape_type,
            row.color,
            round_away(row.start_x),
            round_away(row.start_y),
            round_away(row.end_x),
            round_away(row.end_y),
            simultaneous_events
        )
        .expect("writing to String must succeed");
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EasingFamily, ShapeIdentityAssignment};
    use crate::scene_generation::{
        MotionEvent, MotionEventAccounting, SceneGenerationParams, SceneProjectionMode,
        SceneShapePath, generate_scene,
    };
    use crate::seed_schedule::SceneSeedSchedule;
    use crate::{NormalizedPoint, ShapeFlowConfig};
    use std::collections::{HashMap, HashSet};

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
        let mut config = bootstrap_config();
        config.scene.allow_simultaneous = true;
        config.scene.randomize_motion_events_per_shape = false;
        config.scene.motion_events_per_shape_random_ranges = None;
        config.scene.n_motion_slots = 12;
        config.scene.motion_events_per_shape = vec![4, 4, 4];
        config.scene.n_motion_events_total = Some(12);
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
        assert!(
            [
                "circle", "star", "triangle", "square", "pentagon", "hexagon"
            ]
            .contains(&rows[0].shape_type.as_str())
        );
        let simultaneous_row = rows
            .iter()
            .find(|row| !row.simultaneous_event_ids.is_empty())
            .expect("at least one row should include simultaneous event ids");
        let mut sorted_event_ids = simultaneous_row.simultaneous_event_ids.clone();
        sorted_event_ids.sort_unstable();
        assert_eq!(simultaneous_row.simultaneous_event_ids, sorted_event_ids);
    }

    #[test]
    fn csv_serialization_writes_header_and_rows() {
        let rows = vec![TabularMotionRow {
            scene_id: "00000000000000000000000000000000".to_string(),
            event_index: 0,
            shape_type: "circle".to_string(),
            color: "red".to_string(),
            start_x: -0.25,
            start_y: 0.5,
            end_x: 0.25,
            end_y: -0.5,
            simultaneous_event_ids: vec![12, 7],
        }];
        let csv = serialize_tabular_motion_rows_csv(&rows);

        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "scene_id,event_index,shape_type,color,start_x,start_y,end_x,end_y,simultaneous_events"
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[1],
            "00000000000000000000000000000000,0,circle,red,-0.250,0.500,0.250,-0.500,7|12"
        );
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
            shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
            shape_paths,
            motion_events,
            accounting: MotionEventAccounting {
                expected_total: 4,
                expected_slots: 4,
                generated_total: 4,
                expected_per_shape: vec![1, 2, 1],
                generated_per_shape: vec![1, 2, 1],
            },
        };

        let rows = generate_tabular_motion_rows(&scene).expect("tabular generation should work");
        assert_eq!(rows.len(), 4);

        let row0 = &rows[0];
        assert_eq!(row0.simultaneous_event_ids, vec![1, 2, 3]);

        for row in &rows {
            let self_occurrences = row
                .simultaneous_event_ids
                .iter()
                .filter(|peer| **peer == row.event_index)
                .count();
            assert_eq!(
                self_occurrences, 0,
                "row peers must exclude the event itself"
            );

            let unique_peers = row
                .simultaneous_event_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                unique_peers.len(),
                row.simultaneous_event_ids.len(),
                "peer list must be deduplicated by event ids"
            );
        }
    }

    #[test]
    fn generation_fails_on_invalid_shape_index() {
        let scene = SceneGenerationOutput {
            scene_index: 0,
            schedule: SceneSeedSchedule::derive(1, 0),
            shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
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
                expected_slots: 1,
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

    #[test]
    fn pair_unique_random_mode_supports_repeated_shape_types_and_colors_for_six_shapes() {
        let mut found_seed: Option<u64> = None;
        for scene_index in 0..2_000u64 {
            let scene_layout_seed = SceneSeedSchedule::derive(7, scene_index).scene_layout;

            let mut by_shape_type = HashMap::<usize, HashSet<usize>>::new();
            let mut by_color = HashMap::<usize, HashSet<usize>>::new();
            for shape_index in 0..6 {
                let (shape_type_index, color_index) = shape_type_and_color_indices_for_scene_seed(
                    scene_layout_seed,
                    ShapeIdentityAssignment::PairUniqueRandom,
                    shape_index,
                )
                .expect("pair mode assignment should map each shape index in range");

                by_shape_type
                    .entry(shape_type_index)
                    .or_default()
                    .insert(color_index);
                by_color
                    .entry(color_index)
                    .or_default()
                    .insert(shape_type_index);
            }

            let repeated_shape = by_shape_type.values().any(|colors| colors.len() > 1);
            let repeated_color = by_color.values().any(|shape_types| shape_types.len() > 1);
            if repeated_shape && repeated_color {
                found_seed = Some(scene_index);
                break;
            }
        }

        let seed =
            found_seed.expect("bounded search should find a pair-unique seed with both conditions");
        let scene_layout_seed = SceneSeedSchedule::derive(7, seed).scene_layout;
        let mut by_shape_type = HashMap::<usize, HashSet<usize>>::new();
        let mut by_color = HashMap::<usize, HashSet<usize>>::new();
        for shape_index in 0..6 {
            let (shape_type_index, color_index) = shape_type_and_color_indices_for_scene_seed(
                scene_layout_seed,
                ShapeIdentityAssignment::PairUniqueRandom,
                shape_index,
            )
            .expect("pair mode assignment should map each shape index in range");
            by_shape_type
                .entry(shape_type_index)
                .or_default()
                .insert(color_index);
            by_color
                .entry(color_index)
                .or_default()
                .insert(shape_type_index);
        }

        assert!(by_shape_type.values().any(|colors| colors.len() > 1));
        assert!(by_color.values().any(|shape_types| shape_types.len() > 1));
    }
}
