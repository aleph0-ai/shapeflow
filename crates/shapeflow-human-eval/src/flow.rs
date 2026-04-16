use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use rand::{Rng, SeedableRng, rngs::StdRng};
use shapeflow_core::config::MotionEventsPerShapeRange;
use shapeflow_core::{
    MAX_MASTER_SEED_EXCLUSIVE, ShapeFlowConfig, TextAlterationProfile, TextReferenceFrame,
    canonical_class_rank_for_scene_seed, generate_scene, generate_scene_targets_for_index,
    scene_generation::{SceneGenerationOutput, SceneGenerationParams, SceneProjectionMode},
    tabular_encoding::{COLOR_PALETTE, SHAPE_TYPE_PALETTE, shape_identity_for_scene_seed},
};

pub const PRACTICE_SCENES_PER_MODALITY: usize = 2;
pub const REAL_SCENES_PER_MODALITY: usize = 5;
pub const SCENES_PER_MODALITY_TOTAL: usize =
    PRACTICE_SCENES_PER_MODALITY + REAL_SCENES_PER_MODALITY;
pub const MODALITY_COUNT: usize = 5;
const SCENES_PER_MODALITY: usize = SCENES_PER_MODALITY_TOTAL;
const SAMPLES_PER_EVENT: usize = 24;
const MODALITY_TARGET_SEED_MIX: u64 = 0xA71F_C0DE_5EED_0145;
const MODALITY_ORDER_SHUFFLE_SEED_MIX: u64 = 0x4D4F_4441_4C49_5459;
const LARGEST_EVENT_DISTANCE_TIE_TOLERANCE: f64 = 1.0e-12;

const MODALITY_ORDER: [Modality; MODALITY_COUNT] = [
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

pub type ModalityTargets = [QuestionTarget; MODALITY_COUNT];
pub type ModalityOrder = [usize; MODALITY_COUNT];

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

impl QuestionTarget {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "oqp" => Some(Self::OrderedQuadrantPassage),
            "xct" => Some(Self::CrossingCount),
            "zqh" => Some(Self::QuadrantAfterMoves),
            "lme" => Some(Self::LargestMotionShape),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::OrderedQuadrantPassage => "Ordered Quadrant Passage",
            Self::CrossingCount => "Crossing Count",
            Self::QuadrantAfterMoves => "Quadrant After Moves",
            Self::LargestMotionShape => "Largest Motion Shape",
        }
    }
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
pub struct ShapeChoice {
    pub shape_id: String,
    pub label: String,
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
    /// All shapes present in the scene, for shape-identity selector UI.
    pub scene_shapes: Vec<ShapeChoice>,
    /// Upper bound for integer answers (e.g. crossing count), for slider UI.
    pub integer_max: u32,
}

#[derive(Debug)]
pub enum FlowError {
    InvalidDifficulty(String),
    InvalidSceneIndex(String),
    InvalidModalityIndex(usize),
    InvalidModalityOrder(String),
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
            Self::InvalidModalityOrder(value) => write!(f, "invalid modality order: {value}"),
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

pub fn modality_order_index(modality: Modality) -> usize {
    MODALITY_ORDER
        .iter()
        .position(|m| *m == modality)
        .unwrap_or(0)
}

pub fn modality_from_canonical_index(index: usize) -> Result<Modality, FlowError> {
    MODALITY_ORDER
        .get(index)
        .copied()
        .ok_or(FlowError::InvalidModalityIndex(index))
}

pub fn canonical_modality_order() -> ModalityOrder {
    [0, 1, 2, 3, 4]
}

pub fn modality_order_from_seed(seed: u64) -> ModalityOrder {
    let mut order = canonical_modality_order();
    let mut rng = StdRng::seed_from_u64(seed ^ MODALITY_ORDER_SHUFFLE_SEED_MIX);
    for index in (1..order.len()).rev() {
        let pick = rng.gen_range(0..=index);
        order.swap(index, pick);
    }
    order
}

pub fn serialize_modality_order(order: &ModalityOrder) -> String {
    order
        .iter()
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn parse_modality_order(raw: &str) -> Result<ModalityOrder, FlowError> {
    let values = raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| {
            token
                .parse::<usize>()
                .map_err(|_| FlowError::InvalidModalityOrder(raw.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if values.len() != MODALITY_ORDER.len() {
        return Err(FlowError::InvalidModalityOrder(raw.to_string()));
    }

    let mut seen = [false; MODALITY_COUNT];
    for &value in &values {
        if value >= MODALITY_ORDER.len() || seen[value] {
            return Err(FlowError::InvalidModalityOrder(raw.to_string()));
        }
        seen[value] = true;
    }

    let mut order = [0usize; MODALITY_COUNT];
    for (index, value) in values.into_iter().enumerate() {
        order[index] = value;
    }
    Ok(order)
}

pub fn total_items() -> usize {
    MODALITY_ORDER.len() * SCENES_PER_MODALITY
}

pub fn modality_block_bounds(item_index: usize) -> (usize, usize) {
    let block_start = (item_index / SCENES_PER_MODALITY_TOTAL) * SCENES_PER_MODALITY_TOTAL;
    let block_end = block_start
        .saturating_add(SCENES_PER_MODALITY_TOTAL)
        .min(total_items());
    (block_start.min(total_items()), block_end)
}

pub fn next_modality_start(item_index: usize) -> usize {
    let (_, block_end) = modality_block_bounds(item_index);
    block_end
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
    let (slots, shapes, text_reference_frame, text_synonym_rate, text_typo_rate) = match difficulty
    {
        Difficulty::Easy => (4u32, 3u8, TextReferenceFrame::Canonical, 0.0, 0.0),
        Difficulty::Medium => (8u32, 4u8, TextReferenceFrame::Mixed, 0.20, 0.03),
        Difficulty::Hard => (12u32, 5u8, TextReferenceFrame::Mixed, 0.45, 0.08),
    };

    let min_events = slots / 4;
    let max_events = slots;
    let total_cap = (slots.saturating_mul(3)) / 2;

    let mut config = ShapeFlowConfig::baseline(seed % MAX_MASTER_SEED_EXCLUSIVE);
    config.scene.n_shapes = shapes;
    config.scene.n_motion_slots = slots;
    config.scene.allow_simultaneous = true;
    config.scene.randomize_motion_events_per_shape = true;
    config.scene.motion_events_per_shape = Vec::new();
    config.scene.n_motion_events_total = Some(total_cap);
    config.scene.text_reference_frame = text_reference_frame;
    config.scene.text_synonym_rate = text_synonym_rate;
    config.scene.text_typo_rate = text_typo_rate;

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

pub fn text_alteration_profile_for_difficulty(difficulty: Difficulty) -> TextAlterationProfile {
    match difficulty {
        Difficulty::Easy => TextAlterationProfile::Canonical,
        Difficulty::Medium => TextAlterationProfile::EventClauseReordered,
        Difficulty::Hard => TextAlterationProfile::FullyReordered,
    }
}

pub fn build_plan_item(
    seed: u64,
    difficulty: Difficulty,
    modality_targets: &ModalityTargets,
    item_index: usize,
) -> Result<PlanItem, FlowError> {
    build_plan_item_with_modality_order(
        seed,
        difficulty,
        modality_targets,
        &canonical_modality_order(),
        item_index,
    )
}

pub fn build_plan_item_with_modality_order(
    seed: u64,
    difficulty: Difficulty,
    modality_targets: &ModalityTargets,
    modality_order: &ModalityOrder,
    item_index: usize,
) -> Result<PlanItem, FlowError> {
    let config = build_session_config(seed, difficulty)?;
    build_plan_item_from_config(
        &config,
        seed,
        modality_targets,
        modality_order,
        item_index,
        SAMPLES_PER_EVENT,
    )
}

pub fn build_scene_for_seed(
    seed: u64,
    difficulty: Difficulty,
    scene_index: u32,
) -> Result<SceneGenerationOutput, FlowError> {
    let config = build_session_config(seed, difficulty)?;
    build_scene_for_seed_index(&config, seed, scene_index)
}

pub fn build_scene_for_index(
    config: &ShapeFlowConfig,
    scene_index: u32,
) -> Result<SceneGenerationOutput, FlowError> {
    build_scene_for_absolute_index(config, u64::from(scene_index))
}

fn build_scene_for_seed_index(
    config: &ShapeFlowConfig,
    seed: u64,
    scene_index: u32,
) -> Result<SceneGenerationOutput, FlowError> {
    let effective_index = effective_scene_index(seed, scene_index)?;
    build_scene_for_absolute_index(config, effective_index)
}

fn build_scene_for_absolute_index(
    config: &ShapeFlowConfig,
    scene_index: u64,
) -> Result<SceneGenerationOutput, FlowError> {
    let params = SceneGenerationParams {
        config,
        scene_index,
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
            continue;
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
    seed: u64,
    modality_targets: &ModalityTargets,
    modality_order: &ModalityOrder,
    item_index: usize,
    samples_per_event: usize,
) -> Result<PlanItem, FlowError> {
    let modality_slot = item_index / SCENES_PER_MODALITY;
    let local_index = item_index % SCENES_PER_MODALITY;
    if modality_slot >= MODALITY_ORDER.len() {
        return Err(FlowError::InvalidModalityIndex(modality_slot));
    }

    let canonical_modality_index = modality_order[modality_slot];
    let modality = modality_from_canonical_index(canonical_modality_index)?;
    let target = modality_targets[canonical_modality_index];

    let global_scene_index = modality_slot
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
    let effective_scene_index = effective_scene_index(seed, scene_index)?;
    let scene_layout_seed = config
        .master_seed
        .checked_add(effective_scene_index)
        .ok_or_else(|| {
            FlowError::InvalidSceneIndex(format!(
                "scene index overflow for seed={seed} scene_index={scene_index}"
            ))
        })?;
    let shape_identity_assignment = config.scene.shape_identity_assignment;
    let query_identity = shape_identity_for_scene_seed(
        scene_layout_seed,
        shape_identity_assignment,
        query_shape_index,
    )
    .map_err(|error| FlowError::ShapeIdentity(error.to_string()))?;
    let query_shape_label = shape_id_to_natural_label(&query_identity.shape_id);

    let target_by_id = match target {
        QuestionTarget::LargestMotionShape => None,
        _ => {
            let generated_targets =
                generate_scene_targets_for_index(config, effective_scene_index, samples_per_event)
                    .map_err(|error| FlowError::CoreGeneration(error.to_string()))?;
            Some(
                generated_targets
                    .into_iter()
                    .map(|target| (target.task_id, target.segments))
                    .collect::<BTreeMap<_, _>>(),
            )
        }
    };

    let build_task_id = |prefix: &str| format!("{prefix}{query_shape_index:04}");

    let scene_shapes = (0..shape_count)
        .filter_map(|i| {
            shape_identity_for_scene_seed(scene_layout_seed, shape_identity_assignment, i)
                .ok()
                .map(|id| ShapeChoice {
                    label: shape_id_to_natural_label(&id.shape_id),
                    shape_id: id.shape_id,
                })
        })
        .collect::<Vec<_>>();
    let available_shape_labels = scene_shapes
        .iter()
        .map(|choice| choice.label.clone())
        .collect::<Vec<_>>();

    let (prompt, answer_kind, answer_hint, expected_answer, query_shape) = match target {
        QuestionTarget::OrderedQuadrantPassage => {
            let target_by_id = target_by_id.as_ref().ok_or_else(|| {
                FlowError::CoreGeneration("missing generated targets".to_string())
            })?;
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
            let target_by_id = target_by_id.as_ref().ok_or_else(|| {
                FlowError::CoreGeneration("missing generated targets".to_string())
            })?;
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
            let target_by_id = target_by_id.as_ref().ok_or_else(|| {
                FlowError::CoreGeneration("missing generated targets".to_string())
            })?;
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
            let scene = build_scene_for_absolute_index(config, effective_scene_index)?;
            let mut canonical_ranks = Vec::with_capacity(shape_count);
            for shape_index in 0..shape_count {
                let rank = canonical_class_rank_for_scene_seed(
                    scene_layout_seed,
                    shape_identity_assignment,
                    shape_index,
                )
                .map_err(|error| FlowError::ShapeIdentity(error.to_string()))?;
                canonical_ranks.push(rank);
            }
            let fallback_rank = canonical_ranks.iter().copied().min().unwrap_or(0u8);
            let mut best_distance_squared = -1.0_f64;
            let mut winning_rank = fallback_rank;
            for event in &scene.motion_events {
                let dx = event.end_point.x - event.start_point.x;
                let dy = event.end_point.y - event.start_point.y;
                let distance_squared = dx * dx + dy * dy;
                let rank = canonical_ranks
                    .get(event.shape_index)
                    .copied()
                    .unwrap_or(fallback_rank);

                if distance_squared > best_distance_squared + LARGEST_EVENT_DISTANCE_TIE_TOLERANCE {
                    best_distance_squared = distance_squared;
                    winning_rank = rank;
                    continue;
                }
                if (distance_squared - best_distance_squared).abs()
                    <= LARGEST_EVENT_DISTANCE_TIE_TOLERANCE
                    && rank < winning_rank
                {
                    winning_rank = rank;
                }
            }

            let mut winning_shape_id: Option<String> = None;
            for shape_index in 0..shape_count {
                let rank = canonical_class_rank_for_scene_seed(
                    scene_layout_seed,
                    shape_identity_assignment,
                    shape_index,
                )
                .map_err(|error| FlowError::ShapeIdentity(error.to_string()))?;
                if rank == winning_rank {
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
                format!(
                    "Which shape has traveled the longest distance in a single move between two points?{}",
                    if !available_shape_labels.is_empty() {
                        format!(" Available options: {}.", available_shape_labels.join(", "))
                    } else {
                        String::new()
                    }
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
        scene_shapes,
        integer_max: config.scene.n_motion_slots * 2,
    })
}

fn effective_scene_index(seed: u64, scene_index: u32) -> Result<u64, FlowError> {
    let block = seed / MAX_MASTER_SEED_EXCLUSIVE;
    let base = block
        .checked_mul(MAX_MASTER_SEED_EXCLUSIVE)
        .ok_or_else(|| {
            FlowError::InvalidSceneIndex(format!(
                "scene seed block overflow for seed={seed} scene_index={scene_index}"
            ))
        })?;
    base.checked_add(u64::from(scene_index)).ok_or_else(|| {
        FlowError::InvalidSceneIndex(format!(
            "effective scene index overflow for seed={seed} scene_index={scene_index}"
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_plan_item_lme_does_not_require_target_generation_preconditions() {
        let config = build_session_config(1337, Difficulty::Easy).expect("config should be valid");
        let modality_targets = [
            QuestionTarget::LargestMotionShape,
            QuestionTarget::OrderedQuadrantPassage,
            QuestionTarget::LargestMotionShape,
            QuestionTarget::LargestMotionShape,
            QuestionTarget::LargestMotionShape,
        ];

        let canonical_order = canonical_modality_order();
        let lme_item =
            build_plan_item_from_config(&config, 1337, &modality_targets, &canonical_order, 0, 0)
                .expect("lme items should be independent of target-generation samples");
        assert_eq!(lme_item.target, QuestionTarget::LargestMotionShape);

        let target_item_error = build_plan_item_from_config(
            &config,
            1337,
            &modality_targets,
            &canonical_order,
            SCENES_PER_MODALITY,
            0,
        )
        .expect_err("target tasks should still require target generation samples");
        assert!(
            matches!(target_item_error, FlowError::CoreGeneration(_)),
            "expected core-generation failure when samples_per_event is zero"
        );
    }

    #[test]
    fn largest_motion_shape_prompt_includes_options_and_preserves_expected_answer() {
        let config = build_session_config(1337, Difficulty::Easy).expect("config should be valid");
        let modality_targets = [
            QuestionTarget::LargestMotionShape,
            QuestionTarget::OrderedQuadrantPassage,
            QuestionTarget::LargestMotionShape,
            QuestionTarget::LargestMotionShape,
            QuestionTarget::LargestMotionShape,
        ];
        let canonical_order = canonical_modality_order();
        let item =
            build_plan_item_from_config(&config, 1337, &modality_targets, &canonical_order, 0, 0)
                .expect("lme item should build");

        assert_eq!(item.target, QuestionTarget::LargestMotionShape);
        assert_eq!(item.answer_kind, AnswerKind::ShapeIdentity);

        let expected_options = item
            .scene_shapes
            .iter()
            .map(|shape| shape.label.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let expected_suffix = if expected_options.is_empty() {
            String::new()
        } else {
            format!(" Available options: {}.", expected_options)
        };
        let expected_prompt = format!(
            "Which shape has traveled the longest distance in a single move between two points?{}",
            expected_suffix
        );
        assert_eq!(item.prompt, expected_prompt);
        assert!(
            !item.prompt.contains("Available shapes in this scene:"),
            "shape-identity prompt should only use local options suffix"
        );

        let effective_scene_index =
            effective_scene_index(1337, item.scene_index).expect("mapped scene");
        let scene_layout_seed = config
            .master_seed
            .checked_add(effective_scene_index)
            .expect("effective scene layout seed should fit in u64");
        let scene = build_scene_for_absolute_index(&config, effective_scene_index)
            .expect("scene should build");
        let shape_count = usize::from(config.scene.n_shapes);
        let mut ranks = Vec::with_capacity(shape_count);
        for shape_index in 0..shape_count {
            ranks.push(
                canonical_class_rank_for_scene_seed(
                    scene_layout_seed,
                    config.scene.shape_identity_assignment,
                    shape_index,
                )
                .expect("canonical rank should resolve"),
            );
        }

        let fallback_rank = ranks.iter().copied().min().unwrap_or(0u8);
        let mut best_distance_squared = -1.0_f64;
        let mut winning_rank = fallback_rank;
        for event in &scene.motion_events {
            let dx = event.end_point.x - event.start_point.x;
            let dy = event.end_point.y - event.start_point.y;
            let distance_squared = dx * dx + dy * dy;
            let rank = ranks
                .get(event.shape_index)
                .copied()
                .unwrap_or(fallback_rank);

            if distance_squared > best_distance_squared + LARGEST_EVENT_DISTANCE_TIE_TOLERANCE {
                best_distance_squared = distance_squared;
                winning_rank = rank;
                continue;
            }
            if (distance_squared - best_distance_squared).abs()
                <= LARGEST_EVENT_DISTANCE_TIE_TOLERANCE
                && rank < winning_rank
            {
                winning_rank = rank;
            }
        }

        let expected_shape_id = (0..shape_count)
            .filter_map(|shape_index| {
                canonical_class_rank_for_scene_seed(
                    scene_layout_seed,
                    config.scene.shape_identity_assignment,
                    shape_index,
                )
                .ok()
                .filter(|&rank| rank == winning_rank)
                .and_then(|_| {
                    shape_identity_for_scene_seed(
                        scene_layout_seed,
                        config.scene.shape_identity_assignment,
                        shape_index,
                    )
                    .ok()
                    .map(|identity| identity.shape_id)
                })
            })
            .next()
            .expect("winner should resolve");

        assert_eq!(
            item.expected_answer,
            ExpectedAnswer::ShapeId(expected_shape_id)
        );

        let non_lme_item = build_plan_item_from_config(
            &config,
            1337,
            &modality_targets,
            &canonical_order,
            SCENES_PER_MODALITY,
            24,
        )
        .expect("non-lme item should build");
        assert!(
            !non_lme_item.prompt.contains("Available options:"),
            "non-shape-identity prompts should remain unchanged"
        );
    }

    #[test]
    fn build_session_config_normalizes_master_seed_to_public_range() {
        let seed = (1u64 << 16) + 1337;
        let config = build_session_config(seed, Difficulty::Easy).expect("config should be valid");
        assert_eq!(config.master_seed, 1337);
    }

    #[test]
    fn build_scene_for_seed_uses_effective_scene_index_mapping() {
        let seed = (1u64 << 17) + 42;
        let scene_index = 9u32;
        let config =
            build_session_config(seed, Difficulty::Medium).expect("config should be valid");
        let mapped_scene = build_scene_for_seed(seed, Difficulty::Medium, scene_index)
            .expect("scene should build");
        let expected_scene = build_scene_for_absolute_index(
            &config,
            effective_scene_index(seed, scene_index).expect("effective index should resolve"),
        )
        .expect("direct mapped scene should build");
        assert_eq!(mapped_scene, expected_scene);
    }

    #[test]
    fn difficulty_levels_apply_text_noise_ladder() {
        let easy = build_session_config(7, Difficulty::Easy).expect("easy config should build");
        let medium =
            build_session_config(7, Difficulty::Medium).expect("medium config should build");
        let hard = build_session_config(7, Difficulty::Hard).expect("hard config should build");

        assert_eq!(
            easy.scene.text_reference_frame,
            TextReferenceFrame::Canonical
        );
        assert_eq!(easy.scene.text_synonym_rate, 0.0);
        assert_eq!(easy.scene.text_typo_rate, 0.0);

        assert_eq!(medium.scene.text_reference_frame, TextReferenceFrame::Mixed);
        assert!(medium.scene.text_synonym_rate > easy.scene.text_synonym_rate);
        assert!(medium.scene.text_typo_rate > easy.scene.text_typo_rate);

        assert_eq!(hard.scene.text_reference_frame, TextReferenceFrame::Mixed);
        assert!(hard.scene.text_synonym_rate > medium.scene.text_synonym_rate);
        assert!(hard.scene.text_typo_rate > medium.scene.text_typo_rate);
    }

    #[test]
    fn difficulty_levels_apply_text_alteration_profile_ladder() {
        assert_eq!(
            text_alteration_profile_for_difficulty(Difficulty::Easy),
            TextAlterationProfile::Canonical
        );
        assert_eq!(
            text_alteration_profile_for_difficulty(Difficulty::Medium),
            TextAlterationProfile::EventClauseReordered
        );
        assert_eq!(
            text_alteration_profile_for_difficulty(Difficulty::Hard),
            TextAlterationProfile::FullyReordered
        );
    }

    #[test]
    fn modality_order_roundtrip_and_shape_constraints() {
        let canonical = canonical_modality_order();
        let serialized = serialize_modality_order(&canonical);
        let parsed = parse_modality_order(&serialized).expect("roundtrip should parse");
        assert_eq!(parsed, canonical);

        assert!(parse_modality_order("0,1,2,3").is_err());
        assert!(parse_modality_order("0,1,2,3,5").is_err());
        assert!(parse_modality_order("0,1,1,3,4").is_err());
    }

    #[test]
    fn modality_order_from_seed_is_deterministic_permutation() {
        let first = modality_order_from_seed(12345);
        let second = modality_order_from_seed(12345);
        assert_eq!(first, second);

        let mut seen = [false; MODALITY_COUNT];
        for index in first {
            assert!(index < MODALITY_COUNT);
            assert!(!seen[index]);
            seen[index] = true;
        }
        assert!(seen.into_iter().all(|value| value));
    }

    #[test]
    fn next_modality_start_advances_to_next_block_boundary() {
        assert_eq!(next_modality_start(0), SCENES_PER_MODALITY_TOTAL);
        assert_eq!(
            next_modality_start(SCENES_PER_MODALITY_TOTAL - 1),
            SCENES_PER_MODALITY_TOTAL
        );
        assert_eq!(
            next_modality_start(SCENES_PER_MODALITY_TOTAL),
            SCENES_PER_MODALITY_TOTAL * 2
        );
        assert_eq!(next_modality_start(total_items() - 1), total_items());
        assert_eq!(next_modality_start(total_items()), total_items());
    }

    #[test]
    fn modality_block_bounds_tracks_current_modality_span() {
        assert_eq!(
            modality_block_bounds(SCENES_PER_MODALITY_TOTAL + 2),
            (SCENES_PER_MODALITY_TOTAL, SCENES_PER_MODALITY_TOTAL * 2)
        );
        assert_eq!(
            modality_block_bounds(total_items() - 1),
            (total_items() - SCENES_PER_MODALITY_TOTAL, total_items())
        );
        assert_eq!(
            modality_block_bounds(total_items()),
            (total_items(), total_items())
        );
    }

    #[test]
    fn build_plan_item_with_modality_order_uses_requested_task_order() {
        let seed = 42;
        let difficulty = Difficulty::Easy;
        let modality_targets = modality_targets_from_seed(seed);
        let reversed_order = [4, 3, 2, 1, 0];

        let first_item = build_plan_item_with_modality_order(
            seed,
            difficulty,
            &modality_targets,
            &reversed_order,
            0,
        )
        .expect("first item should build");
        assert_eq!(first_item.modality, Modality::Sound);

        let second_block_item = build_plan_item_with_modality_order(
            seed,
            difficulty,
            &modality_targets,
            &reversed_order,
            SCENES_PER_MODALITY,
        )
        .expect("second block item should build");
        assert_eq!(second_block_item.modality, Modality::Tabular);
    }

    #[test]
    fn plan_item_query_shape_matches_generated_scene_across_difficulties_and_modalities() {
        let seeds = [42_u64, (1_u64 << 16) + 1337, (1_u64 << 17) + 42];
        let difficulties = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

        for seed in seeds {
            let modality_targets = modality_targets_from_seed(seed);
            for difficulty in difficulties {
                for item_index in 0..total_items() {
                    let item = build_plan_item(seed, difficulty, &modality_targets, item_index)
                        .expect("plan item should build");
                    let Some(query_shape_id) = item.query_shape.as_ref() else {
                        continue;
                    };

                    let scene = build_scene_for_seed(seed, difficulty, item.scene_index)
                        .expect("scene should build for plan item");
                    let has_query_shape = (0..scene.shape_paths.len()).any(|shape_index| {
                        shapeflow_core::tabular_encoding::shape_identity_for_scene(
                            &scene,
                            shape_index,
                        )
                        .map(|identity| identity.shape_id == *query_shape_id)
                        .unwrap_or(false)
                    });

                    assert!(
                        has_query_shape,
                        "query shape '{query_shape_id}' missing from scene: seed={seed} difficulty={difficulty:?} item_index={item_index} scene_index={}",
                        item.scene_index
                    );
                }
            }
        }
    }
}
