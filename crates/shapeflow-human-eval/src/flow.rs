use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use rand::{Rng, SeedableRng, rngs::StdRng};
use shapeflow_core::config::MotionEventsPerShapeRange;
use shapeflow_core::{
    ShapeFlowConfig, canonical_class_rank_for_scene_seed, generate_scene,
    generate_scene_targets_for_index,
    scene_generation::{SceneGenerationOutput, SceneGenerationParams, SceneProjectionMode},
    tabular_encoding::{COLOR_PALETTE, SHAPE_TYPE_PALETTE, shape_identity_for_scene_seed},
};

pub const PRACTICE_SCENES_PER_MODALITY: usize = 2;
pub const REAL_SCENES_PER_MODALITY: usize = 5;
const SCENES_PER_MODALITY: usize = PRACTICE_SCENES_PER_MODALITY + REAL_SCENES_PER_MODALITY;
const SAMPLES_PER_EVENT: usize = 24;
const MODALITY_TARGET_SEED_MIX: u64 = 0xA71F_C0DE_5EED_0145;

const MODALITY_ORDER: [Modality; 5] = [
    Modality::Image,
    Modality::Video,
    Modality::Text,
    Modality::Tabular,
    Modality::Sound,
];

const QUESTION_TARGETS: [QuestionTarget; 4] = [
    QuestionTarget::OrderedQuadrantPassage,
    QuestionTarget::CrossingCount,
    QuestionTarget::QuadrantAfterMoves,
    QuestionTarget::LargestMotionShape,
];

pub type ModalityTargets = [QuestionTarget; 5];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    Image,
    Video,
    Text,
    Tabular,
    Sound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionTarget {
    OrderedQuadrantPassage,
    CrossingCount,
    QuadrantAfterMoves,
    LargestMotionShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedAnswer {
    Sequence(Vec<usize>),
    Integer(i64),
    Quadrant(usize),
    ShapeId(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerKind {
    QuadrantSequence,
    Integer,
    Quadrant,
    ShapeIdentity,
}

#[derive(Debug, Clone)]
pub struct PlanItem {
    pub item_index: usize,
    pub modality: Modality,
    pub scene_index: u32,
    pub is_practice: bool,
    pub target: QuestionTarget,
    pub query_shape: Option<String>,
    pub prompt: String,
    pub answer_kind: AnswerKind,
    pub answer_hint: String,
    pub expected_answer: ExpectedAnswer,
}

#[derive(Debug)]
pub enum FlowError {
    InvalidDifficulty(String),
    InvalidSceneIndex(String),
    InvalidModalityIndex(usize),
    InvalidFormAnswer(String),
    CoreConfig(String),
    CoreGeneration(String),
    ShapeIdentity(String),
    TargetMissingTask { task_id: String },
    TargetPayloadMalformed { task_id: String },
    LargestShapeTargetResolution,
}

impl Display for FlowError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDifficulty(value) => {
                write!(f, "invalid difficulty '{value}', expected easy|medium|hard")
            }
            Self::InvalidSceneIndex(value) => write!(f, "invalid scene index {value}"),
            Self::InvalidModalityIndex(index) => write!(f, "invalid modality index {index}"),
            Self::InvalidFormAnswer(value) => {
                write!(f, "could not parse answer '{value}'")
            }
            Self::CoreConfig(message) => write!(f, "{message}"),
            Self::CoreGeneration(message) => write!(f, "{message}"),
            Self::ShapeIdentity(message) => write!(f, "{message}"),
            Self::TargetMissingTask { task_id } => {
                write!(f, "missing generated target task_id={task_id}")
            }
            Self::TargetPayloadMalformed { task_id } => {
                write!(
                    f,
                    "malformed generated target payload for task_id={task_id}"
                )
            }
            Self::LargestShapeTargetResolution => {
                write!(f, "could not resolve largest-motion target to shape id")
            }
        }
    }
}

impl std::error::Error for FlowError {}

impl Difficulty {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Easy => "easy",
            Self::Medium => "medium",
            Self::Hard => "hard",
        }
    }

    pub fn from_str(value: &str) -> Result<Self, FlowError> {
        match value {
            "easy" => Ok(Self::Easy),
            "medium" => Ok(Self::Medium),
            "hard" => Ok(Self::Hard),
            other => Err(FlowError::InvalidDifficulty(other.to_string())),
        }
    }
}

impl Modality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Text => "text",
            Self::Tabular => "tabular",
            Self::Sound => "sound",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::Video => "Video",
            Self::Text => "Text",
            Self::Tabular => "Tabular",
            Self::Sound => "Sound",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            "text" => Some(Self::Text),
            "tabular" => Some(Self::Tabular),
            "sound" => Some(Self::Sound),
            _ => None,
        }
    }
}

impl QuestionTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrderedQuadrantPassage => "oqp",
            Self::CrossingCount => "xct",
            Self::QuadrantAfterMoves => "zqh",
            Self::LargestMotionShape => "lme",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::OrderedQuadrantPassage => "Ordered quadrant passage",
            Self::CrossingCount => "Quadrant crossing count",
            Self::QuadrantAfterMoves => "Quadrant after moves",
            Self::LargestMotionShape => "Largest single motion shape",
        }
    }
}

impl AnswerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QuadrantSequence => "quadrant_sequence",
            Self::Integer => "integer",
            Self::Quadrant => "quadrant",
            Self::ShapeIdentity => "shape_identity",
        }
    }
}

pub fn total_items() -> usize {
    MODALITY_ORDER.len() * SCENES_PER_MODALITY
}

pub fn scenes_per_modality() -> usize {
    SCENES_PER_MODALITY
}

pub fn task_number(item_index: usize) -> usize {
    item_index / SCENES_PER_MODALITY + 1
}

pub fn local_item_index(item_index: usize) -> usize {
    item_index % SCENES_PER_MODALITY
}

pub fn modality_targets_from_seed(seed: u64) -> ModalityTargets {
    let mut rng = StdRng::seed_from_u64(seed ^ MODALITY_TARGET_SEED_MIX);
    let mut targets = [QuestionTarget::OrderedQuadrantPassage; MODALITY_ORDER.len()];
    for target in &mut targets {
        let index = rng.gen_range(0..QUESTION_TARGETS.len());
        *target = QUESTION_TARGETS[index];
    }
    targets
}

pub fn build_session_config(
    seed: u64,
    difficulty: Difficulty,
) -> Result<ShapeFlowConfig, FlowError> {
    let (slots, shapes) = match difficulty {
        Difficulty::Easy => (4u32, 3u8),
        Difficulty::Medium => (8u32, 4u8),
        Difficulty::Hard => (12u32, 5u8),
    };

    let min_events = slots / 4;
    let max_events = slots;
    let total_cap = (slots.saturating_mul(3)) / 2;

    let mut config = ShapeFlowConfig::baseline(seed);
    config.scene.n_shapes = shapes;
    config.scene.n_motion_slots = slots;
    config.scene.allow_simultaneous = true;
    config.scene.randomize_motion_events_per_shape = true;
    config.scene.motion_events_per_shape = Vec::new();
    config.scene.n_motion_events_total = Some(total_cap);

    config.scene.motion_events_per_shape_random_ranges = Some(vec![
        MotionEventsPerShapeRange {
            min: min_events as u16,
            max: max_events as u16,
        };
        usize::from(shapes)
    ]);

    config
        .validate()
        .map_err(|error| FlowError::CoreConfig(error.to_string()))?;

    Ok(config)
}

pub fn build_plan_item(
    seed: u64,
    difficulty: Difficulty,
    modality_targets: &ModalityTargets,
    item_index: usize,
) -> Result<PlanItem, FlowError> {
    let config = build_session_config(seed, difficulty)?;
    build_plan_item_from_config(&config, modality_targets, item_index)
}

pub fn build_scene_for_seed(
    seed: u64,
    difficulty: Difficulty,
    scene_index: u32,
) -> Result<SceneGenerationOutput, FlowError> {
    let config = build_session_config(seed, difficulty)?;
    build_scene_for_index(&config, scene_index)
}

pub fn build_scene_for_index(
    config: &ShapeFlowConfig,
    scene_index: u32,
) -> Result<SceneGenerationOutput, FlowError> {
    let params = SceneGenerationParams {
        config,
        scene_index: u64::from(scene_index),
        samples_per_event: SAMPLES_PER_EVENT,
        projection: SceneProjectionMode::SoftQuadrants,
    };
    generate_scene(&params).map_err(|error| FlowError::CoreGeneration(error.to_string()))
}

pub fn is_practice_item(item_index: usize) -> bool {
    let local = item_index % SCENES_PER_MODALITY;
    local < PRACTICE_SCENES_PER_MODALITY
}

pub fn canonical_sequence(sequence: &[usize]) -> String {
    sequence
        .iter()
        .map(|quadrant| quadrant.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn format_expected_answer(expected: &ExpectedAnswer) -> String {
    match expected {
        ExpectedAnswer::Sequence(sequence) => canonical_sequence(sequence),
        ExpectedAnswer::Integer(value) => value.to_string(),
        ExpectedAnswer::Quadrant(quadrant) => quadrant.to_string(),
        ExpectedAnswer::ShapeId(shape_id) => shape_id_to_natural_label(shape_id),
    }
}

pub fn parse_sequence(raw: &str) -> Result<Vec<usize>, FlowError> {
    let mut out = Vec::new();
    for token in raw.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err(FlowError::InvalidFormAnswer(raw.to_string()));
        }
        let quadrant = token
            .parse::<usize>()
            .map_err(|_| FlowError::InvalidFormAnswer(raw.to_string()))?;
        if !(1..=4).contains(&quadrant) {
            return Err(FlowError::InvalidFormAnswer(raw.to_string()));
        }
        out.push(quadrant);
    }

    if out.is_empty() {
        return Err(FlowError::InvalidFormAnswer(raw.to_string()));
    }

    Ok(out)
}

pub fn parse_integer(raw: &str) -> Result<i64, FlowError> {
    let trimmed = raw.trim();
    trimmed
        .parse::<i64>()
        .map_err(|_| FlowError::InvalidFormAnswer(raw.to_string()))
}

pub fn parse_quadrant(raw: &str) -> Result<usize, FlowError> {
    let sequence = parse_sequence(raw)?;
    if sequence.len() != 1 {
        return Err(FlowError::InvalidFormAnswer(raw.to_string()));
    }
    Ok(sequence[0])
}

pub fn normalize_shape_id(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub fn parse_shape_answer(raw: &str) -> Option<String> {
    let flat = normalize_shape_id(raw);
    if flat.is_empty() {
        return None;
    }

    for shape in SHAPE_TYPE_PALETTE {
        for color in COLOR_PALETTE {
            if flat == format!("{shape}{color}") || flat == format!("{color}{shape}") {
                return Some(format!("{shape}_{color}"));
            }
        }
    }

    let tokens = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphabetic() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(|token| token.to_string())
        .collect::<Vec<_>>();

    let shape = tokens
        .iter()
        .find(|token| SHAPE_TYPE_PALETTE.contains(&token.as_str()))
        .cloned();
    let color = tokens
        .iter()
        .find(|token| COLOR_PALETTE.contains(&token.as_str()))
        .cloned();

    match (shape, color) {
        (Some(shape), Some(color)) => Some(format!("{shape}_{color}")),
        _ => None,
    }
}

pub fn shape_id_to_natural_label(shape_id: &str) -> String {
    let mut split = shape_id.split('_');
    let shape = split.next();
    let color = split.next();
    if split.next().is_none() {
        if let (Some(shape), Some(color)) = (shape, color) {
            return format!("{color} {shape}");
        }
    }
    shape_id.replace('_', " ")
}

fn build_plan_item_from_config(
    config: &ShapeFlowConfig,
    modality_targets: &ModalityTargets,
    item_index: usize,
) -> Result<PlanItem, FlowError> {
    let modality_index = item_index / SCENES_PER_MODALITY;
    let local_index = item_index % SCENES_PER_MODALITY;
    if modality_index >= MODALITY_ORDER.len() {
        return Err(FlowError::InvalidModalityIndex(modality_index));
    }

    let modality = MODALITY_ORDER[modality_index];
    let target = modality_targets[modality_index];

    let global_scene_index = modality_index
        .saturating_mul(SCENES_PER_MODALITY)
        .saturating_add(local_index);
    let scene_index = u32::try_from(global_scene_index).map_err(|_| {
        FlowError::InvalidSceneIndex(format!("item {item_index} has non-convertible scene index"))
    })?;

    let shape_count = usize::from(config.scene.n_shapes);
    if shape_count == 0 {
        return Err(FlowError::CoreConfig(
            "invalid config shape count: zero".to_string(),
        ));
    }

    let query_shape_index = local_index % shape_count;
    let scene_layout_seed = config.master_seed + u64::from(scene_index);
    let shape_identity_assignment = config.scene.shape_identity_assignment;
    let query_identity = shape_identity_for_scene_seed(
        scene_layout_seed,
        shape_identity_assignment,
        query_shape_index,
    )
    .map_err(|error| FlowError::ShapeIdentity(error.to_string()))?;
    let query_shape_label = shape_id_to_natural_label(&query_identity.shape_id);

    let generated_targets =
        generate_scene_targets_for_index(config, u64::from(scene_index), SAMPLES_PER_EVENT)
            .map_err(|error| FlowError::CoreGeneration(error.to_string()))?;
    let target_by_id = generated_targets
        .into_iter()
        .map(|target| (target.task_id, target.segments))
        .collect::<BTreeMap<_, _>>();

    let build_task_id = |prefix: &str| format!("{prefix}{query_shape_index:04}");

    let (prompt, answer_kind, answer_hint, expected_answer, query_shape) = match target {
        QuestionTarget::OrderedQuadrantPassage => {
            let task_id = build_task_id("oqp");
            let segments =
                target_by_id
                    .get(&task_id)
                    .ok_or_else(|| FlowError::TargetMissingTask {
                        task_id: task_id.clone(),
                    })?;
            let mut sequence = Vec::with_capacity(segments.len());
            for segment in segments {
                if segment.len() != 4 {
                    return Err(FlowError::TargetPayloadMalformed {
                        task_id: task_id.clone(),
                    });
                }
                sequence.push(dominant_quadrant(segment) + 1);
            }
            (
                format!(
                    "For the {}, list the ordered quadrant passage.",
                    query_shape_label
                ),
                AnswerKind::QuadrantSequence,
                String::from("Format: comma-separated quadrants (e.g. 1,3,4)"),
                ExpectedAnswer::Sequence(sequence),
                Some(query_identity.shape_id),
            )
        }
        QuestionTarget::CrossingCount => {
            let task_id = build_task_id("xct");
            let value = scalar_target_value(&target_by_id, &task_id)?;
            (
                format!(
                    "For the {}, how many total quadrant crossings occurred?",
                    query_shape_label
                ),
                AnswerKind::Integer,
                String::from("Enter an integer (e.g. 3)"),
                ExpectedAnswer::Integer(value),
                Some(query_identity.shape_id),
            )
        }
        QuestionTarget::QuadrantAfterMoves => {
            let task_id = build_task_id("zqh");
            let (move_count, quadrant_zero_based) = zqh_target_values(&target_by_id, &task_id)?;
            (
                format!(
                    "For the {}, what quadrant is it in after it moved {} times?",
                    query_shape_label, move_count
                ),
                AnswerKind::Quadrant,
                String::from("Enter one quadrant number: 1, 2, 3, or 4"),
                ExpectedAnswer::Quadrant(quadrant_zero_based + 1),
                Some(query_identity.shape_id),
            )
        }
        QuestionTarget::LargestMotionShape => {
            let task_id = String::from("lme0000");
            let winning_rank = scalar_target_value(&target_by_id, &task_id)?;
            let winning_rank_u8 =
                u8::try_from(winning_rank).map_err(|_| FlowError::TargetPayloadMalformed {
                    task_id: task_id.clone(),
                })?;

            let mut winning_shape_id: Option<String> = None;
            for shape_index in 0..shape_count {
                let rank = canonical_class_rank_for_scene_seed(
                    scene_layout_seed,
                    shape_identity_assignment,
                    shape_index,
                )
                .map_err(|error| FlowError::ShapeIdentity(error.to_string()))?;
                if rank == winning_rank_u8 {
                    let identity = shape_identity_for_scene_seed(
                        scene_layout_seed,
                        shape_identity_assignment,
                        shape_index,
                    )
                    .map_err(|error| FlowError::ShapeIdentity(error.to_string()))?;
                    winning_shape_id = Some(identity.shape_id);
                    break;
                }
            }

            let winning_shape_id =
                winning_shape_id.ok_or(FlowError::LargestShapeTargetResolution)?;
            (
                String::from(
                    "Which shape has traveled the longest distance in a single move between two points?",
                ),
                AnswerKind::ShapeIdentity,
                String::from("Enter shape and color (e.g. red circle)"),
                ExpectedAnswer::ShapeId(winning_shape_id),
                None,
            )
        }
    };

    Ok(PlanItem {
        item_index,
        modality,
        scene_index,
        is_practice: local_index < PRACTICE_SCENES_PER_MODALITY,
        target,
        query_shape,
        prompt,
        answer_kind,
        answer_hint,
        expected_answer,
    })
}

fn scalar_target_value(
    target_by_id: &BTreeMap<String, Vec<Vec<f64>>>,
    task_id: &str,
) -> Result<i64, FlowError> {
    let segments = target_by_id
        .get(task_id)
        .ok_or_else(|| FlowError::TargetMissingTask {
            task_id: task_id.to_string(),
        })?;
    let value = segments
        .first()
        .and_then(|segment| segment.first())
        .copied()
        .ok_or_else(|| FlowError::TargetPayloadMalformed {
            task_id: task_id.to_string(),
        })?;
    if !value.is_finite() {
        return Err(FlowError::TargetPayloadMalformed {
            task_id: task_id.to_string(),
        });
    }
    Ok(value.round() as i64)
}

fn zqh_target_values(
    target_by_id: &BTreeMap<String, Vec<Vec<f64>>>,
    task_id: &str,
) -> Result<(usize, usize), FlowError> {
    let segments = target_by_id
        .get(task_id)
        .ok_or_else(|| FlowError::TargetMissingTask {
            task_id: task_id.to_string(),
        })?;
    let segment = segments
        .first()
        .ok_or_else(|| FlowError::TargetPayloadMalformed {
            task_id: task_id.to_string(),
        })?;
    if segment.len() != 2 || !segment[0].is_finite() || !segment[1].is_finite() {
        return Err(FlowError::TargetPayloadMalformed {
            task_id: task_id.to_string(),
        });
    }

    let move_count = segment[0].round() as i64;
    let quadrant = segment[1].round() as i64;
    if move_count < 1 || !(0..=3).contains(&quadrant) {
        return Err(FlowError::TargetPayloadMalformed {
            task_id: task_id.to_string(),
        });
    }

    Ok((move_count as usize, quadrant as usize))
}

fn dominant_quadrant(values: &[f64]) -> usize {
    let mut winner = 0usize;
    let mut winner_value = values[0];

    for (index, value) in values.iter().copied().enumerate().skip(1) {
        if value > winner_value {
            winner = index;
            winner_value = value;
        }
    }

    winner
}
