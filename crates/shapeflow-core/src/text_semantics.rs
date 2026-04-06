use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use rand::{Rng, RngCore};
use rand_chacha::ChaCha8Rng;

use crate::config::{EasingFamily, SceneConfig, TextReferenceFrame};
use crate::scene_generation::SceneGenerationOutput;
use crate::tabular_encoding::shape_identity_for_scene;
use crate::trajectory::NormalizedPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlterationProfile {
    Canonical,
    EventClauseReordered,
    PairClauseReordered,
    FullyReordered,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSurfaceOptions {
    pub text_reference_frame: TextReferenceFrame,
    pub text_synonym_rate: f64,
    pub text_typo_rate: f64,
}

impl Default for TextSurfaceOptions {
    fn default() -> Self {
        Self {
            text_reference_frame: TextReferenceFrame::Canonical,
            text_synonym_rate: 0.0,
            text_typo_rate: 0.0,
        }
    }
}

impl TextSurfaceOptions {
    pub fn from_scene_config(scene_cfg: &SceneConfig) -> Self {
        Self {
            text_reference_frame: scene_cfg.text_reference_frame,
            text_synonym_rate: scene_cfg.text_synonym_rate,
            text_typo_rate: scene_cfg.text_typo_rate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalSemanticRelation {
    LeftOf,
    RightOf,
    AlignedHorizontally,
}

impl HorizontalSemanticRelation {
    fn as_phrase(self) -> &'static str {
        match self {
            Self::LeftOf => "left of",
            Self::RightOf => "right of",
            Self::AlignedHorizontally => "aligned horizontally with",
        }
    }

    fn parse(phrase: &str) -> Option<Self> {
        match phrase {
            "to the left of" => Some(Self::LeftOf),
            "left of" => Some(Self::LeftOf),
            "leftward of" => Some(Self::LeftOf),
            "to the right of" => Some(Self::RightOf),
            "right of" => Some(Self::RightOf),
            "aligned horizontally with" => Some(Self::AlignedHorizontally),
            "horizontally aligned with" => Some(Self::AlignedHorizontally),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalSemanticRelation {
    Above,
    Below,
    AlignedVertically,
}

impl VerticalSemanticRelation {
    fn as_phrase(self) -> &'static str {
        match self {
            Self::Above => "above",
            Self::Below => "below",
            Self::AlignedVertically => "aligned vertically with",
        }
    }

    fn parse(phrase: &str) -> Option<Self> {
        match phrase {
            "above" => Some(Self::Above),
            "below" => Some(Self::Below),
            "aligned vertically with" => Some(Self::AlignedVertically),
            "vertically aligned with" => Some(Self::AlignedVertically),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventSemanticFrame {
    pub event_index: u32,
    pub shape_id: String,
    pub start_point: NormalizedPoint,
    pub end_point: NormalizedPoint,
    pub duration_frames: u16,
    pub easing: EasingFamily,
    pub simultaneous_with: Vec<SimultaneousEventRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimultaneousEventRef {
    pub event_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quadrant {
    TopRight,
    TopLeft,
    BottomLeft,
    BottomRight,
}

impl Quadrant {
    fn as_phrase(self) -> &'static str {
        match self {
            Self::TopRight => "top right",
            Self::TopLeft => "top left",
            Self::BottomLeft => "bottom left",
            Self::BottomRight => "bottom right",
        }
    }

    fn center_point(self) -> NormalizedPoint {
        match self {
            Self::TopRight => {
                NormalizedPoint::new(0.5, 0.5).expect("top-right center must be normalized")
            }
            Self::TopLeft => {
                NormalizedPoint::new(-0.5, 0.5).expect("top-left center must be normalized")
            }
            Self::BottomLeft => {
                NormalizedPoint::new(-0.5, -0.5).expect("bottom-left center must be normalized")
            }
            Self::BottomRight => {
                NormalizedPoint::new(0.5, -0.5).expect("bottom-right center must be normalized")
            }
        }
    }

    fn parse(phrase: &str) -> Option<Self> {
        match phrase.trim() {
            "top right" => Some(Self::TopRight),
            "top left" => Some(Self::TopLeft),
            "bottom left" => Some(Self::BottomLeft),
            "bottom right" => Some(Self::BottomRight),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairSemanticFrame {
    pub pair_index: usize,
    pub event_index: u32,
    pub first_shape_id: String,
    pub second_shape_id: String,
    pub horizontal_relation: HorizontalSemanticRelation,
    pub vertical_relation: VerticalSemanticRelation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneTextSemantics {
    pub scene_index: u64,
    pub events: Vec<EventSemanticFrame>,
    pub pairs: Vec<PairSemanticFrame>,
}

#[derive(Debug, thiserror::Error)]
pub enum TextSemanticsError {
    #[error("shape identity generation failed: {0}")]
    ShapeIdentity(String),

    #[error("shape index {shape_index} out of bounds for scene with {shape_count} shapes")]
    ShapeIndexOutOfBounds {
        shape_index: usize,
        shape_count: usize,
    },

    #[error("shape {shape_id} (index {shape_index}) missing an anchor trajectory point")]
    MissingAnchorPoint {
        shape_id: String,
        shape_index: usize,
    },

    #[error("failed to parse text line: {line}")]
    ParseLine { line: String },

    #[error("failed to parse relation phrase: {phrase}")]
    ParseRelation { phrase: String },

    #[error("failed to parse easing family token: {token}")]
    ParseEasing { token: String },

    #[error("event index mismatch: expected contiguous index {expected}, got {actual}")]
    EventIndexMismatch { expected: usize, actual: u32 },

    #[error("pair index mismatch: expected contiguous index {expected}, got {actual}")]
    PairIndexMismatch { expected: usize, actual: usize },

    #[error("scene header event count mismatch: header={header_count}, parsed={parsed_count}")]
    HeaderEventCountMismatch {
        header_count: usize,
        parsed_count: usize,
    },
}

fn pair_sentence_count(shape_count: usize) -> usize {
    shape_count * shape_count.saturating_sub(1) / 2
}

pub fn derive_scene_text_semantics(
    scene: &SceneGenerationOutput,
) -> Result<SceneTextSemantics, TextSemanticsError> {
    let shape_count = scene.shape_paths.len();
    let mut shape_ids = Vec::with_capacity(shape_count);
    let mut anchors = Vec::with_capacity(shape_count);

    for shape_index in 0..shape_count {
        let identity = shape_identity_for_scene(scene, shape_index)
            .map_err(|error| TextSemanticsError::ShapeIdentity(error.to_string()))?;
        let anchor_point = scene
            .shape_paths
            .get(shape_index)
            .and_then(|path| path.trajectory_points.first().copied())
            .ok_or_else(|| TextSemanticsError::MissingAnchorPoint {
                shape_id: identity.shape_id.clone(),
                shape_index,
            })?;
        shape_ids.push(identity.shape_id);
        anchors.push(anchor_point);
    }

    let mut time_slot_events: BTreeMap<u32, Vec<(u32, usize)>> = BTreeMap::new();
    for event in &scene.motion_events {
        if event.shape_index >= shape_count {
            return Err(TextSemanticsError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        time_slot_events
            .entry(event.time_slot)
            .or_default()
            .push((event.global_event_index, event.shape_index));
    }

    let mut ordered_events = scene.motion_events.clone();
    ordered_events.sort_by_key(|event| event.global_event_index);

    let mut events = Vec::with_capacity(ordered_events.len());
    for event in &ordered_events {
        let mut simultaneous_with = time_slot_events
            .get(&event.time_slot)
            .expect("time slot should exist for every event")
            .iter()
            .filter(|(peer_event_index, _)| *peer_event_index != event.global_event_index)
            .map(|(peer_event_index, _)| SimultaneousEventRef {
                event_index: *peer_event_index,
            })
            .collect::<Vec<_>>();
        simultaneous_with.sort_by_key(|peer| peer.event_index);
        events.push(EventSemanticFrame {
            event_index: event.global_event_index,
            shape_id: shape_ids[event.shape_index].clone(),
            start_point: event.start_point,
            end_point: event.end_point,
            duration_frames: event.duration_frames,
            easing: event.easing,
            simultaneous_with,
        });
    }

    let pairs_per_event = pair_sentence_count(shape_count);
    let mut pairs = Vec::with_capacity(ordered_events.len().saturating_mul(pairs_per_event));
    let mut current_positions = anchors.clone();
    let mut pair_index = 0usize;
    for event in &ordered_events {
        current_positions[event.shape_index] = event.end_point;
        for first_shape in 0..shape_count {
            for second_shape in (first_shape + 1)..shape_count {
                let first_position = current_positions[first_shape];
                let second_position = current_positions[second_shape];
                pairs.push(PairSemanticFrame {
                    pair_index,
                    event_index: event.global_event_index,
                    first_shape_id: shape_ids[first_shape].clone(),
                    second_shape_id: shape_ids[second_shape].clone(),
                    horizontal_relation: horizontal_relation(first_position, second_position),
                    vertical_relation: vertical_relation(first_position, second_position),
                });
                pair_index += 1;
            }
        }
    }

    Ok(SceneTextSemantics {
        scene_index: scene.scene_index,
        events,
        pairs,
    })
}

pub fn generate_scene_text_lines_with_alteration(
    scene: &SceneGenerationOutput,
    profile: TextAlterationProfile,
) -> Result<Vec<String>, TextSemanticsError> {
    let mut grammar_rng = scene.schedule.text_grammar_rng();
    let mut lexical_rng = scene.schedule.lexical_noise_rng();
    let semantics = derive_scene_text_semantics(scene)?;
    Ok(render_scene_text_lines_from_semantics(
        &semantics,
        profile,
        TextSurfaceOptions::default(),
        &mut grammar_rng,
        &mut lexical_rng,
    ))
}

pub fn generate_scene_text_lines_with_scene_config(
    scene: &SceneGenerationOutput,
    scene_cfg: &SceneConfig,
) -> Result<Vec<String>, TextSemanticsError> {
    generate_scene_text_lines_with_scene_config_and_alteration(
        scene,
        scene_cfg,
        TextAlterationProfile::Canonical,
    )
}

pub fn generate_scene_text_lines_with_scene_config_and_alteration(
    scene: &SceneGenerationOutput,
    scene_cfg: &SceneConfig,
    profile: TextAlterationProfile,
) -> Result<Vec<String>, TextSemanticsError> {
    let mut grammar_rng = scene.schedule.text_grammar_rng();
    let mut lexical_rng = scene.schedule.lexical_noise_rng();
    let semantics = derive_scene_text_semantics(scene)?;
    let options = TextSurfaceOptions::from_scene_config(scene_cfg);
    Ok(render_scene_text_lines_from_semantics(
        &semantics,
        profile,
        options,
        &mut grammar_rng,
        &mut lexical_rng,
    ))
}

pub fn decode_scene_text_semantics(
    lines: &[String],
) -> Result<SceneTextSemantics, TextSemanticsError> {
    let (scene_index, header_count, content_lines) = parse_header(lines)?;
    let mut events = Vec::new();
    let mut pairs = Vec::new();

    for raw_line in content_lines {
        let line = normalize_surface_variants(raw_line);
        if line.starts_with("Event ") {
            events.push(parse_event_line(&line)?);
        } else if line.starts_with("Pair ") {
            pairs.push(parse_pair_line(&line)?);
        } else {
            return Err(TextSemanticsError::ParseLine {
                line: raw_line.to_string(),
            });
        }
    }

    events.sort_by_key(|event| event.event_index);
    for (expected, event) in events.iter().enumerate() {
        let actual = event.event_index;
        if usize::try_from(actual).ok() != Some(expected) {
            return Err(TextSemanticsError::EventIndexMismatch { expected, actual });
        }
    }

    pairs.sort_by_key(|pair| pair.pair_index);
    for (expected, pair) in pairs.iter().enumerate() {
        if pair.pair_index != expected {
            return Err(TextSemanticsError::PairIndexMismatch {
                expected,
                actual: pair.pair_index,
            });
        }
    }

    if header_count != events.len() {
        return Err(TextSemanticsError::HeaderEventCountMismatch {
            header_count,
            parsed_count: events.len(),
        });
    }

    Ok(SceneTextSemantics {
        scene_index,
        events,
        pairs,
    })
}

fn render_scene_text_lines_from_semantics(
    semantics: &SceneTextSemantics,
    profile: TextAlterationProfile,
    options: TextSurfaceOptions,
    grammar_rng: &mut ChaCha8Rng,
    lexical_rng: &mut ChaCha8Rng,
) -> Vec<String> {
    let mut lines = Vec::with_capacity(semantics.events.len() + semantics.pairs.len() + 1);
    lines.push(format!(
        "Scene {:032x}: {} motion events.",
        semantics.scene_index,
        semantics.events.len()
    ));

    for event in &semantics.events {
        let reference_frame = choose_reference_frame(options.text_reference_frame, grammar_rng);
        let mut line = render_event_line(event, profile, reference_frame);
        line = apply_text_surface_variation(
            line,
            options.text_synonym_rate,
            options.text_typo_rate,
            lexical_rng,
        );
        lines.push(line);
    }

    for pair in &semantics.pairs {
        let mut line = render_pair_line(pair, profile);
        line = apply_text_surface_variation(
            line,
            options.text_synonym_rate,
            options.text_typo_rate,
            lexical_rng,
        );
        lines.push(line);
    }

    lines
}

fn render_event_line(
    event: &EventSemanticFrame,
    profile: TextAlterationProfile,
    reference_frame: TextReferenceFrame,
) -> String {
    let reference_suffix = if reference_frame == TextReferenceFrame::Relative {
        " relative to the scene center"
    } else {
        ""
    };
    let movement_clause = render_event_movement_clause(event);
    let shape_label = render_shape_label(&event.shape_id);
    let simultaneous_clause = render_simultaneous_clause(&event.simultaneous_with);

    match profile {
        TextAlterationProfile::Canonical | TextAlterationProfile::PairClauseReordered => {
            if let Some(simultaneous_clause) = simultaneous_clause {
                format!(
                    "Event {:04}: {} moved {}{}. This happened at the same time as {}.",
                    event.event_index,
                    shape_label,
                    movement_clause,
                    reference_suffix,
                    simultaneous_clause
                )
            } else {
                format!(
                    "Event {:04}: {} moved {}{}.",
                    event.event_index, shape_label, movement_clause, reference_suffix
                )
            }
        }
        TextAlterationProfile::EventClauseReordered | TextAlterationProfile::FullyReordered => {
            if let Some(simultaneous_clause) = simultaneous_clause {
                format!(
                    "Event {:04}: At the same time as {}, {} moved {}{}.",
                    event.event_index,
                    simultaneous_clause,
                    shape_label,
                    movement_clause,
                    reference_suffix
                )
            } else {
                format!(
                    "Event {:04}: {} moved {}{}.",
                    event.event_index, shape_label, movement_clause, reference_suffix
                )
            }
        }
    }
}

fn render_event_movement_clause(event: &EventSemanticFrame) -> String {
    let path = sampled_quadrant_path(event);
    if path.is_empty() {
        return "within top left quadrant".to_string();
    }

    if path.len() == 1 {
        return format!("within {} quadrant", path[0].as_phrase());
    }

    let start = path.first().expect("non-empty path");
    let end = path.last().expect("non-empty path");
    let through = &path[1..path.len() - 1];
    if through.is_empty() {
        format!(
            "from {} quadrant to {} quadrant",
            start.as_phrase(),
            end.as_phrase()
        )
    } else {
        format!(
            "from {} quadrant to {} quadrant through {}",
            start.as_phrase(),
            end.as_phrase(),
            render_quadrant_list(through)
        )
    }
}

fn render_quadrant_list(quadrants: &[Quadrant]) -> String {
    let parts = quadrants
        .iter()
        .map(|quadrant| format!("{} quadrant", quadrant.as_phrase()))
        .collect::<Vec<_>>();
    render_list_with_and(&parts)
}

fn render_shape_label(shape_id: &str) -> String {
    if let Some((shape_type, color)) = shape_id.split_once('_') {
        return format!("{} {} shape", color, shape_type);
    }
    shape_id.to_string()
}

fn parse_shape_label(label: &str) -> Option<String> {
    let label = label.trim();
    let label = label.strip_suffix(" shape")?;
    let mut parts = label.split_whitespace();
    let color = parts.next()?;
    let shape_type = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{shape_type}_{color}"))
}

fn render_simultaneous_clause(simultaneous_with: &[SimultaneousEventRef]) -> Option<String> {
    if simultaneous_with.is_empty() {
        return None;
    }
    let mut event_indices = simultaneous_with
        .iter()
        .map(|peer| peer.event_index)
        .collect::<Vec<_>>();
    event_indices.sort_unstable();
    let parts = event_indices
        .into_iter()
        .map(|event_index| format!("Event {:04}", event_index))
        .collect::<Vec<_>>();
    Some(render_list_with_and(&parts))
}

fn render_list_with_and(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let mut body = items[..items.len() - 1].join(", ");
            body.push_str(", and ");
            body.push_str(&items[items.len() - 1]);
            body
        }
    }
}

fn sampled_quadrant_path(event: &EventSemanticFrame) -> Vec<Quadrant> {
    let sample_count = usize::from(event.duration_frames.max(2));
    let mut path = Vec::new();
    for index in 0..sample_count {
        let t = if sample_count <= 1 {
            1.0
        } else {
            index as f64 / (sample_count - 1) as f64
        };
        let eased = easing_progress(t, event.easing);
        let x = lerp(event.start_point.x, event.end_point.x, eased);
        let y = lerp(event.start_point.y, event.end_point.y, eased);
        let quadrant = dominant_quadrant(x, y);
        if path.last().copied() != Some(quadrant) {
            path.push(quadrant);
        }
    }
    path
}

fn dominant_quadrant(x: f64, y: f64) -> Quadrant {
    if x >= 0.0 && y >= 0.0 {
        Quadrant::TopRight
    } else if x < 0.0 && y >= 0.0 {
        Quadrant::TopLeft
    } else if x < 0.0 && y < 0.0 {
        Quadrant::BottomLeft
    } else {
        Quadrant::BottomRight
    }
}

fn lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

fn easing_progress(progress: f64, easing: EasingFamily) -> f64 {
    match easing {
        EasingFamily::Linear => progress,
        EasingFamily::EaseIn => progress * progress,
        EasingFamily::EaseOut => 1.0 - (1.0 - progress) * (1.0 - progress),
        EasingFamily::EaseInOut => {
            if progress < 0.5 {
                2.0 * progress * progress
            } else {
                1.0 - ((-2.0 * progress + 2.0) * (-2.0 * progress + 2.0)) / 2.0
            }
        }
    }
}

fn render_pair_line(pair: &PairSemanticFrame, profile: TextAlterationProfile) -> String {
    match profile {
        TextAlterationProfile::Canonical | TextAlterationProfile::EventClauseReordered => format!(
            "Pair {:04}: [event {:04}] {} is {} {} and is {} {}.",
            pair.pair_index,
            pair.event_index,
            pair.first_shape_id,
            pair.horizontal_relation.as_phrase(),
            pair.second_shape_id,
            pair.vertical_relation.as_phrase(),
            pair.second_shape_id
        ),
        TextAlterationProfile::PairClauseReordered | TextAlterationProfile::FullyReordered => {
            format!(
                "Pair {:04}: [event {:04}] {} is {} {} and is {} {}.",
                pair.pair_index,
                pair.event_index,
                pair.first_shape_id,
                pair.vertical_relation.as_phrase(),
                pair.second_shape_id,
                pair.horizontal_relation.as_phrase(),
                pair.second_shape_id
            )
        }
    }
}

fn choose_reference_frame(
    reference_frame: TextReferenceFrame,
    grammar_rng: &mut ChaCha8Rng,
) -> TextReferenceFrame {
    match reference_frame {
        TextReferenceFrame::Canonical | TextReferenceFrame::Relative => reference_frame,
        TextReferenceFrame::Mixed => {
            if should_apply_from_probability(0.5, grammar_rng) {
                TextReferenceFrame::Relative
            } else {
                TextReferenceFrame::Canonical
            }
        }
    }
}

fn apply_text_surface_variation(
    line: String,
    synonym_rate: f64,
    typo_rate: f64,
    lexical_rng: &mut ChaCha8Rng,
) -> String {
    let line = apply_per_word_synonyms(line, synonym_rate, lexical_rng);
    apply_per_word_keyboard_typos(line, typo_rate, lexical_rng)
}

const SYNONYM_CANONICAL_TOKENS: [&str; 5] = ["moved", "happened", "same", "relative", "scene"];

const TYPO_KEYBOARD_RADIUS: usize = 1;

const QWERTY_NEIGHBOR_GROUPS: [(char, &str); 26] = [
    ('q', "12wsa"),
    ('w', "q23edsa"),
    ('e', "w34rfds"),
    ('r', "e45tfd"),
    ('t', "r56ygf"),
    ('y', "67uhgt"),
    ('u', "78ijhy"),
    ('i', "89okju"),
    ('o', "90plki"),
    ('p', "0-[';lo"),
    ('a', "qwsz<"),
    ('s', "aqwedxz"),
    ('d', "erfcxsw"),
    ('f', "ertgvcd"),
    ('g', "tyhbvf"),
    ('h', "yujnbg"),
    ('j', "uikmnh"),
    ('k', "iol,mj"),
    ('l', "op;.,k"),
    ('z', "asx<"),
    ('x', "zsdc "),
    ('c', "xdfv "),
    ('v', "cfgb "),
    ('b', "vghn "),
    ('n', "bhjm "),
    ('m', "njk, "),
];

const TYPO_CORRECTION_VOCAB: [&str; 50] = [
    "Scene",
    "Event",
    "Pair",
    "At",
    "This",
    "the",
    "as",
    "at",
    "of",
    "scene",
    "motion",
    "event",
    "events",
    "red",
    "green",
    "blue",
    "yellow",
    "magenta",
    "cyan",
    "circle",
    "star",
    "triangle",
    "square",
    "pentagon",
    "hexagon",
    "shape",
    "moved",
    "translated",
    "within",
    "from",
    "to",
    "quadrant",
    "through",
    "top",
    "bottom",
    "left",
    "right",
    "above",
    "below",
    "aligned",
    "horizontally",
    "vertically",
    "with",
    "relative",
    "center",
    "same",
    "time",
    "happened",
    "and",
    "is",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypoClass {
    Doubling,
    Omission,
    Replacement,
    AdditionAfter,
    AdditionBefore,
    PairSwap,
}

fn apply_per_word_synonyms(line: String, probability: f64, rng: &mut ChaCha8Rng) -> String {
    if probability <= 0.0 || !probability.is_finite() {
        return line;
    }

    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;

    while index < line.len() {
        let ch = line[index..]
            .chars()
            .next()
            .expect("index must stay in bounds");
        if ch.is_ascii_alphabetic() {
            let start = index;
            index += ch.len_utf8();
            while index < line.len() {
                let next = line[index..]
                    .chars()
                    .next()
                    .expect("index must stay in bounds while scanning token");
                if next.is_ascii_alphabetic() {
                    index += next.len_utf8();
                } else {
                    break;
                }
            }

            let token = &line[start..index];
            let canonical = token.to_ascii_lowercase();
            let replacement = synonym_variants_by_canonical()
                .get(&canonical)
                .filter(|choices| !choices.is_empty());
            if let Some(choices) = replacement {
                if should_apply_from_probability(probability, rng) {
                    let choice = rng.gen_range(0..choices.len());
                    output.push_str(&match_token_case(token, &choices[choice]));
                } else {
                    output.push_str(token);
                }
            } else {
                output.push_str(token);
            }
        } else {
            output.push(ch);
            index += ch.len_utf8();
        }
    }

    output
}

fn synonym_variants_by_canonical() -> &'static BTreeMap<String, Vec<String>> {
    static MAP: OnceLock<BTreeMap<String, Vec<String>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let dict = thesaurus::dict();
        let mut out = BTreeMap::<String, Vec<String>>::new();
        for canonical in SYNONYM_CANONICAL_TOKENS {
            let Some(raw) = dict.get(canonical) else {
                continue;
            };
            let canonical_lower = canonical.to_ascii_lowercase();
            let mut variants = raw
                .iter()
                .filter_map(|candidate| normalize_synonym_candidate(candidate))
                .filter(|candidate| candidate != &canonical_lower)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if !variants.is_empty() {
                variants.sort();
                out.insert(canonical_lower, variants);
            }
        }
        out
    })
}

fn synonym_canonical_by_variant() -> &'static BTreeMap<String, String> {
    static MAP: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut out = BTreeMap::<String, String>::new();
        for (canonical, variants) in synonym_variants_by_canonical() {
            for variant in variants {
                out.entry(variant.clone())
                    .or_insert_with(|| canonical.clone());
            }
        }
        out
    })
}

fn normalize_synonym_candidate(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower
        .chars()
        .all(|ch| ch.is_ascii_alphabetic())
        .then_some(lower)
}

fn match_token_case(template: &str, replacement_lower: &str) -> String {
    if template.chars().all(|ch| ch.is_ascii_uppercase()) {
        replacement_lower.to_ascii_uppercase()
    } else {
        let mut chars = template.chars();
        if let Some(first) = chars.next()
            && first.is_ascii_uppercase()
            && chars.all(|ch| ch.is_ascii_lowercase())
        {
            let mut replacement = replacement_lower.to_string();
            if let Some(first_byte) = replacement.get_mut(0..1) {
                first_byte.make_ascii_uppercase();
            }
            return replacement;
        }
        replacement_lower.to_string()
    }
}

fn apply_per_word_keyboard_typos(line: String, probability: f64, rng: &mut ChaCha8Rng) -> String {
    if probability <= 0.0 || !probability.is_finite() {
        return line;
    }

    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;
    while index < line.len() {
        let ch = line[index..]
            .chars()
            .next()
            .expect("index must stay in bounds");
        if ch.is_ascii_alphabetic() {
            let start = index;
            index += ch.len_utf8();
            while index < line.len() {
                let next = line[index..]
                    .chars()
                    .next()
                    .expect("index must stay in bounds while scanning token");
                if next.is_ascii_alphabetic() {
                    index += next.len_utf8();
                } else {
                    break;
                }
            }
            let token = &line[start..index];
            let token_len = token.chars().count();
            if token_len >= 4
                && !is_typo_protected_token(token)
                && should_apply_from_probability(probability, rng)
            {
                if let Some(typo) = apply_random_keyboard_typo(token, TYPO_KEYBOARD_RADIUS, rng) {
                    output.push_str(&typo);
                } else {
                    output.push_str(token);
                }
            } else {
                output.push_str(token);
            }
        } else {
            output.push(ch);
            index += ch.len_utf8();
        }
    }
    output
}

fn is_typo_protected_token(token: &str) -> bool {
    let token_lower = token.to_ascii_lowercase();
    matches!(token_lower.as_str(), "scene" | "event" | "pair")
        || synonym_variants_by_canonical().contains_key(&token_lower)
        || synonym_canonical_by_variant().contains_key(&token_lower)
}

fn apply_random_keyboard_typo(token: &str, radius: usize, rng: &mut ChaCha8Rng) -> Option<String> {
    let token_chars = token.chars().collect::<Vec<_>>();
    if token_chars.is_empty() {
        return None;
    }

    let mut classes = [
        TypoClass::Doubling,
        TypoClass::Omission,
        TypoClass::Replacement,
        TypoClass::AdditionAfter,
        TypoClass::AdditionBefore,
        TypoClass::PairSwap,
    ];
    for index in (1..classes.len()).rev() {
        let swap_with = rng.gen_range(0..=index);
        classes.swap(index, swap_with);
    }

    for class in classes {
        if let Some(variant) = apply_keyboard_typo_class(&token_chars, class, radius, rng) {
            if variant != token {
                return Some(variant);
            }
        }
    }
    None
}

fn apply_keyboard_typo_class(
    token_chars: &[char],
    class: TypoClass,
    radius: usize,
    rng: &mut ChaCha8Rng,
) -> Option<String> {
    let len = token_chars.len();
    match class {
        TypoClass::Doubling => {
            let index = rng.gen_range(0..len);
            let mut chars = token_chars.to_vec();
            chars.insert(index + 1, token_chars[index]);
            Some(chars.into_iter().collect())
        }
        TypoClass::Omission => {
            if len < 2 {
                return None;
            }
            let index = rng.gen_range(0..len);
            let mut chars = token_chars.to_vec();
            chars.remove(index);
            Some(chars.into_iter().collect())
        }
        TypoClass::PairSwap => {
            if len < 2 {
                return None;
            }
            let index = rng.gen_range(0..(len - 1));
            let mut chars = token_chars.to_vec();
            chars.swap(index, index + 1);
            Some(chars.into_iter().collect())
        }
        TypoClass::Replacement => {
            let candidates = token_chars
                .iter()
                .enumerate()
                .filter_map(|(index, ch)| {
                    let neighbors = keyboard_neighbors_for_char(*ch, radius);
                    (!neighbors.is_empty()).then_some((index, neighbors))
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return None;
            }
            let choice = rng.gen_range(0..candidates.len());
            let (index, neighbors) = &candidates[choice];
            let neighbor = neighbors[rng.gen_range(0..neighbors.len())];
            let mut chars = token_chars.to_vec();
            chars[*index] = neighbor;
            Some(chars.into_iter().collect())
        }
        TypoClass::AdditionAfter | TypoClass::AdditionBefore => {
            let candidates = token_chars
                .iter()
                .enumerate()
                .filter_map(|(index, ch)| {
                    let neighbors = keyboard_neighbors_for_char(*ch, radius);
                    (!neighbors.is_empty()).then_some((index, neighbors))
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return None;
            }
            let choice = rng.gen_range(0..candidates.len());
            let (index, neighbors) = &candidates[choice];
            let neighbor = neighbors[rng.gen_range(0..neighbors.len())];
            let mut chars = token_chars.to_vec();
            let insert_index = match class {
                TypoClass::AdditionAfter => *index + 1,
                TypoClass::AdditionBefore => *index,
                _ => unreachable!("handled by match arm"),
            };
            chars.insert(insert_index, neighbor);
            Some(chars.into_iter().collect())
        }
    }
}

fn keyboard_neighbors_for_char(base: char, radius: usize) -> Vec<char> {
    if !base.is_ascii_alphabetic() || radius == 0 {
        return Vec::new();
    }
    let base_lower = base.to_ascii_lowercase();
    let mut visited = std::collections::BTreeSet::<char>::new();
    let mut queue = std::collections::VecDeque::<(char, usize)>::new();
    visited.insert(base_lower);
    queue.push_back((base_lower, 0));

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= radius {
            continue;
        }
        for neighbor in immediate_keyboard_neighbors(current) {
            if visited.insert(neighbor) {
                queue.push_back((neighbor, depth + 1));
            }
        }
    }

    visited.remove(&base_lower);
    let mut out = Vec::with_capacity(visited.len());
    for mut ch in visited {
        if base.is_ascii_uppercase() {
            ch = ch.to_ascii_uppercase();
        }
        out.push(ch);
    }
    out
}

fn immediate_keyboard_neighbors(base_lower: char) -> Vec<char> {
    let mut out = Vec::<char>::new();

    if let Some((_, neighbors)) = QWERTY_NEIGHBOR_GROUPS
        .iter()
        .find(|(key, _)| *key == base_lower)
    {
        for neighbor in neighbors.chars() {
            if neighbor.is_ascii_alphabetic() && !out.contains(&neighbor) {
                out.push(neighbor);
            }
        }
    }

    for (key, neighbors) in QWERTY_NEIGHBOR_GROUPS {
        if neighbors.chars().any(|candidate| candidate == base_lower)
            && key.is_ascii_alphabetic()
            && !out.contains(&key)
        {
            out.push(key);
        }
    }

    out
}

fn normalize_surface_variants(line: &str) -> String {
    let mut corrected = String::with_capacity(line.len());
    let mut index = 0usize;
    while index < line.len() {
        let ch = line[index..]
            .chars()
            .next()
            .expect("index must stay in bounds");
        if ch.is_ascii_alphabetic() {
            let start = index;
            index += ch.len_utf8();
            while index < line.len() {
                let next = line[index..]
                    .chars()
                    .next()
                    .expect("index must stay in bounds while scanning token");
                if next.is_ascii_alphabetic() {
                    index += next.len_utf8();
                } else {
                    break;
                }
            }
            let token = &line[start..index];
            let token = normalize_typo_token(token);
            corrected.push_str(&normalize_synonym_token(&token));
        } else {
            corrected.push(ch);
            index += ch.len_utf8();
        }
    }

    let mut normalized = corrected;
    if normalized.contains("translated") {
        normalized = normalized.replace("translated", "moved");
    }
    normalized
}

fn normalize_typo_token(token: &str) -> String {
    if TYPO_CORRECTION_VOCAB.contains(&token) {
        return token.to_string();
    }

    let candidates = TYPO_CORRECTION_VOCAB
        .iter()
        .copied()
        .filter(|canonical| is_single_keyboard_typo_variant(canonical, token, TYPO_KEYBOARD_RADIUS))
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        return candidates[0].to_string();
    }

    token.to_string()
}

fn normalize_synonym_token(token: &str) -> String {
    let lower = token.to_ascii_lowercase();
    if let Some(canonical) = synonym_canonical_by_variant().get(&lower) {
        return match_token_case(token, canonical);
    }
    token.to_string()
}

fn is_single_keyboard_typo_variant(canonical: &str, observed: &str, radius: usize) -> bool {
    if canonical == observed {
        return false;
    }
    let canonical_chars = canonical.chars().collect::<Vec<_>>();
    let observed_chars = observed.chars().collect::<Vec<_>>();

    if canonical_chars.is_empty() || observed_chars.is_empty() {
        return false;
    }

    // Class 1: doubling
    for index in 0..canonical_chars.len() {
        let mut candidate = canonical_chars.clone();
        candidate.insert(index + 1, canonical_chars[index]);
        if candidate == observed_chars {
            return true;
        }
    }

    // Class 2: omission
    if canonical_chars.len() >= 2 && observed_chars.len() + 1 == canonical_chars.len() {
        for index in 0..canonical_chars.len() {
            let mut candidate = canonical_chars.clone();
            candidate.remove(index);
            if candidate == observed_chars {
                return true;
            }
        }
    }

    // Class 3: replacement
    if canonical_chars.len() == observed_chars.len() {
        for index in 0..canonical_chars.len() {
            if canonical_chars[index] == observed_chars[index] {
                continue;
            }
            let neighbors = keyboard_neighbors_for_char(canonical_chars[index], radius);
            if neighbors.contains(&observed_chars[index]) {
                let mut candidate = canonical_chars.clone();
                candidate[index] = observed_chars[index];
                if candidate == observed_chars {
                    return true;
                }
            }
        }
    }

    // Classes 4 and 5: insertion after / insertion before
    if observed_chars.len() == canonical_chars.len() + 1 {
        for index in 0..canonical_chars.len() {
            let neighbors = keyboard_neighbors_for_char(canonical_chars[index], radius);
            for neighbor in &neighbors {
                let mut after = canonical_chars.clone();
                after.insert(index + 1, *neighbor);
                if after == observed_chars {
                    return true;
                }
                let mut before = canonical_chars.clone();
                before.insert(index, *neighbor);
                if before == observed_chars {
                    return true;
                }
            }
        }
    }

    // Class 6: pair swap
    if canonical_chars.len() >= 2 && canonical_chars.len() == observed_chars.len() {
        for index in 0..(canonical_chars.len() - 1) {
            let mut candidate = canonical_chars.clone();
            candidate.swap(index, index + 1);
            if candidate == observed_chars {
                return true;
            }
        }
    }

    false
}

fn should_apply_from_probability(probability: f64, rng: &mut ChaCha8Rng) -> bool {
    if probability <= 0.0 || !probability.is_finite() {
        false
    } else if probability >= 1.0 {
        true
    } else {
        (rng.next_u64() as f64) / (u64::MAX as f64) < probability
    }
}

fn parse_header(lines: &[String]) -> Result<(u64, usize, &[String]), TextSemanticsError> {
    let header = lines.first().ok_or_else(|| TextSemanticsError::ParseLine {
        line: String::new(),
    })?;
    if !header.starts_with("Scene ") {
        return Err(TextSemanticsError::ParseLine {
            line: header.clone(),
        });
    }

    let body = &header["Scene ".len()..];
    let (scene_hex, event_tail) =
        body.split_once(": ")
            .ok_or_else(|| TextSemanticsError::ParseLine {
                line: header.clone(),
            })?;
    let event_count_text = event_tail.strip_suffix(" motion events.").ok_or_else(|| {
        TextSemanticsError::ParseLine {
            line: header.clone(),
        }
    })?;
    let scene_index =
        u64::from_str_radix(scene_hex, 16).map_err(|_| TextSemanticsError::ParseLine {
            line: header.clone(),
        })?;
    let header_count =
        event_count_text
            .parse::<usize>()
            .map_err(|_| TextSemanticsError::ParseLine {
                line: header.clone(),
            })?;

    Ok((scene_index, header_count, &lines[1..]))
}

fn parse_event_line(line: &str) -> Result<EventSemanticFrame, TextSemanticsError> {
    let (event_index, body) = parse_indexed_line_prefix(line, "Event")?;
    let body = body.trim();
    let mut simultaneous_with = Vec::new();

    let (body, prefix_simultaneous_clause) =
        if let Some(rest) = body.strip_prefix("At the same time as ") {
            let (simultaneous_text, tail) =
                rest.split_once(", ")
                    .ok_or_else(|| TextSemanticsError::ParseLine {
                        line: line.to_string(),
                    })?;
            (tail, Some(simultaneous_text))
        } else {
            (body, None)
        };

    let body = body.trim_end_matches('.');
    let (movement_clause, suffix_simultaneous_clause) =
        if let Some((head, tail)) = body.split_once(". This happened at the same time as ") {
            (head, Some(tail))
        } else {
            (body, None)
        };

    if let Some(text) = prefix_simultaneous_clause {
        simultaneous_with.extend(parse_simultaneous_clause(text, line)?);
    }
    if let Some(text) = suffix_simultaneous_clause {
        simultaneous_with.extend(parse_simultaneous_clause(text, line)?);
    }
    simultaneous_with.sort_by_key(|peer| peer.event_index);
    simultaneous_with.dedup_by_key(|peer| peer.event_index);

    let movement_clause = movement_clause
        .trim_end_matches(" relative to the scene center")
        .trim();
    let (shape_label, movement_description) =
        movement_clause
            .split_once(" moved ")
            .ok_or_else(|| TextSemanticsError::ParseLine {
                line: line.to_string(),
            })?;
    let shape_id = parse_shape_label(shape_label).ok_or_else(|| TextSemanticsError::ParseLine {
        line: line.to_string(),
    })?;
    let (start_quadrant, end_quadrant) = parse_movement_description(movement_description, line)?;

    Ok(EventSemanticFrame {
        event_index: u32::try_from(event_index).map_err(|_| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?,
        shape_id,
        start_point: start_quadrant.center_point(),
        end_point: end_quadrant.center_point(),
        duration_frames: 0,
        easing: EasingFamily::Linear,
        simultaneous_with,
    })
}

fn parse_pair_line(line: &str) -> Result<PairSemanticFrame, TextSemanticsError> {
    let (pair_index, body) = parse_indexed_line_prefix(line, "Pair")?;

    let relation_text = body
        .strip_suffix('.')
        .ok_or_else(|| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;
    let (event_marker, relation_body) =
        relation_text
            .split_once("] ")
            .ok_or_else(|| TextSemanticsError::ParseLine {
                line: line.to_string(),
            })?;
    let event_index_text =
        event_marker
            .strip_prefix("[event ")
            .ok_or_else(|| TextSemanticsError::ParseLine {
                line: line.to_string(),
            })?;
    let event_index =
        event_index_text
            .parse::<u32>()
            .map_err(|_| TextSemanticsError::ParseLine {
                line: line.to_string(),
            })?;

    let (subject_part, tail) =
        relation_body
            .split_once(" is ")
            .ok_or_else(|| TextSemanticsError::ParseLine {
                line: line.to_string(),
            })?;
    let (horizontal_relation_segment, vertical_relation_segment) = tail
        .split_once(" and is ")
        .ok_or_else(|| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;

    let (horizontal_phrase, horizontal_object) =
        split_relation_with_object(horizontal_relation_segment).ok_or_else(|| {
            TextSemanticsError::ParseLine {
                line: line.to_string(),
            }
        })?;
    let (vertical_phrase, vertical_object) = split_relation_with_object(vertical_relation_segment)
        .ok_or_else(|| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;

    if horizontal_object != vertical_object {
        return Err(TextSemanticsError::ParseLine {
            line: line.to_string(),
        });
    }
    let horizontal_relation = HorizontalSemanticRelation::parse(horizontal_phrase)
        .or_else(|| HorizontalSemanticRelation::parse(vertical_phrase))
        .ok_or_else(|| TextSemanticsError::ParseRelation {
            phrase: horizontal_phrase.to_string(),
        })?;
    let vertical_relation = VerticalSemanticRelation::parse(horizontal_phrase)
        .or_else(|| VerticalSemanticRelation::parse(vertical_phrase))
        .ok_or_else(|| TextSemanticsError::ParseRelation {
            phrase: vertical_phrase.to_string(),
        })?;

    Ok(PairSemanticFrame {
        pair_index,
        event_index,
        first_shape_id: subject_part.to_string(),
        second_shape_id: horizontal_object.to_string(),
        horizontal_relation,
        vertical_relation,
    })
}

fn split_relation_with_object(relation: &str) -> Option<(&str, &str)> {
    let (phrase, object) = relation.rsplit_once(' ')?;
    Some((phrase, object))
}

fn parse_indexed_line_prefix<'a>(
    line: &'a str,
    prefix: &str,
) -> Result<(usize, &'a str), TextSemanticsError> {
    let expected_prefix = format!("{prefix} ");
    if !line.starts_with(&expected_prefix) {
        return Err(TextSemanticsError::ParseLine {
            line: line.to_string(),
        });
    }

    let after_prefix = &line[expected_prefix.len()..];
    let (index_text, body) =
        after_prefix
            .split_once(": ")
            .ok_or_else(|| TextSemanticsError::ParseLine {
                line: line.to_string(),
            })?;
    let index = index_text
        .parse::<usize>()
        .map_err(|_| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;

    Ok((index, body))
}

fn parse_movement_description(
    text: &str,
    line: &str,
) -> Result<(Quadrant, Quadrant), TextSemanticsError> {
    if let Some(within) = text.strip_prefix("within ") {
        let quadrant = within
            .strip_suffix(" quadrant")
            .and_then(Quadrant::parse)
            .ok_or_else(|| TextSemanticsError::ParseLine {
                line: line.to_string(),
            })?;
        return Ok((quadrant, quadrant));
    }

    let from_to = text
        .strip_prefix("from ")
        .ok_or_else(|| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;
    let (from_phrase, to_tail) =
        from_to
            .split_once(" quadrant to ")
            .ok_or_else(|| TextSemanticsError::ParseLine {
                line: line.to_string(),
            })?;
    let to_phrase =
        if let Some((to_phrase, _through_tail)) = to_tail.split_once(" quadrant through ") {
            to_phrase
        } else {
            to_tail
                .strip_suffix(" quadrant")
                .ok_or_else(|| TextSemanticsError::ParseLine {
                    line: line.to_string(),
                })?
        };
    let from_quadrant =
        Quadrant::parse(from_phrase).ok_or_else(|| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;
    let to_quadrant = Quadrant::parse(to_phrase).ok_or_else(|| TextSemanticsError::ParseLine {
        line: line.to_string(),
    })?;
    Ok((from_quadrant, to_quadrant))
}

fn parse_simultaneous_clause(
    text: &str,
    line: &str,
) -> Result<Vec<SimultaneousEventRef>, TextSemanticsError> {
    let peers_text = text.trim().trim_end_matches('.').trim();
    if peers_text.is_empty() {
        return Ok(Vec::new());
    }
    let normalized = peers_text.replace(", and ", ", ").replace(" and ", ", ");
    let mut peers = normalized
        .split(',')
        .filter(|peer| !peer.trim().is_empty())
        .map(|peer| {
            let peer = peer.trim();
            let peer =
                peer.strip_prefix("Event ")
                    .ok_or_else(|| TextSemanticsError::ParseLine {
                        line: line.to_string(),
                    })?;
            let event_index = peer
                .parse::<u32>()
                .map_err(|_| TextSemanticsError::ParseLine {
                    line: line.to_string(),
                })?;
            Ok(SimultaneousEventRef { event_index })
        })
        .collect::<Result<Vec<_>, _>>()?;
    peers.sort_unstable_by_key(|peer| peer.event_index);
    Ok(peers)
}

fn horizontal_relation(
    left: NormalizedPoint,
    right: NormalizedPoint,
) -> HorizontalSemanticRelation {
    if left.x < right.x {
        HorizontalSemanticRelation::LeftOf
    } else if left.x > right.x {
        HorizontalSemanticRelation::RightOf
    } else {
        HorizontalSemanticRelation::AlignedHorizontally
    }
}

fn vertical_relation(top: NormalizedPoint, bottom: NormalizedPoint) -> VerticalSemanticRelation {
    if top.y > bottom.y {
        VerticalSemanticRelation::Above
    } else if top.y < bottom.y {
        VerticalSemanticRelation::Below
    } else {
        VerticalSemanticRelation::AlignedVertically
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShapeFlowConfig;
    use crate::scene_generation::{SceneGenerationParams, SceneProjectionMode, generate_scene};
    use rand::SeedableRng;

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config should parse")
    }

    #[test]
    fn alteration_profiles_decode_to_same_semantics() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 2,
            samples_per_event: 16,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");
        let canonical_lines =
            generate_scene_text_lines_with_alteration(&scene, TextAlterationProfile::Canonical)
                .expect("canonical line generation should work");
        for line in &canonical_lines {
            if line.starts_with("Event ") {
                assert!(
                    !line.contains(" over "),
                    "canonical event line unexpectedly contained duration phrase: {line}"
                );
                assert!(
                    !line.contains(" using "),
                    "canonical event line unexpectedly contained easing phrase: {line}"
                );
                assert!(
                    line.contains("quadrant"),
                    "canonical event line should include quadrant wording: {line}"
                );
            }
        }
        let canonical_semantics =
            decode_scene_text_semantics(&canonical_lines).expect("canonical decode should work");
        assert_eq!(canonical_semantics.scene_index, scene.scene_index);
        assert_eq!(canonical_semantics.events.len(), scene.motion_events.len());
        assert_eq!(
            canonical_semantics.pairs.len(),
            scene.motion_events.len() * pair_sentence_count(scene.shape_paths.len())
        );

        let profiles = [
            TextAlterationProfile::Canonical,
            TextAlterationProfile::EventClauseReordered,
            TextAlterationProfile::PairClauseReordered,
            TextAlterationProfile::FullyReordered,
        ];

        for profile in profiles {
            let lines = generate_scene_text_lines_with_alteration(&scene, profile)
                .expect("line generation should work");
            let decoded = decode_scene_text_semantics(&lines).expect("decode should work");
            assert_eq!(decoded, canonical_semantics);
        }
    }

    #[test]
    fn alteration_profiles_have_distinct_surface_forms() {
        let config = bootstrap_config();
        let params = SceneGenerationParams {
            config: &config,
            scene_index: 0,
            samples_per_event: 8,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params).expect("scene generation should succeed");

        let canonical =
            generate_scene_text_lines_with_alteration(&scene, TextAlterationProfile::Canonical)
                .expect("canonical generation should work");
        let reordered = generate_scene_text_lines_with_alteration(
            &scene,
            TextAlterationProfile::FullyReordered,
        )
        .expect("reordered generation should work");

        assert_ne!(canonical, reordered);
        assert_eq!(canonical.len(), reordered.len());
    }

    #[test]
    fn decode_rejects_unknown_line_kind() {
        let lines = vec![
            "Scene 00000000000000000000000000000000: 0 motion events.".to_string(),
            "Unknown 0000: nonsense".to_string(),
        ];
        assert!(decode_scene_text_semantics(&lines).is_err());
    }

    #[test]
    fn text_surface_variation_applies_synonyms_per_word() {
        let moved_choices = synonym_variants_by_canonical()
            .get("moved")
            .expect("moved should have thesaurus variants configured");
        assert!(!moved_choices.is_empty());

        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let line = "Event 0000: shape moved, then moved again; moved.".to_string();
        let varied = apply_text_surface_variation(line.clone(), 1.0, 0.0, &mut rng);
        assert_ne!(varied, line);
        let normalized = normalize_surface_variants(&varied);
        assert_eq!(normalized, line);
    }

    #[test]
    fn text_surface_variation_applies_keyboard_typos_per_word() {
        let mut rng = ChaCha8Rng::seed_from_u64(9);
        let line = "Event 0000: red circle shape moved.".to_string();
        let varied = apply_text_surface_variation(line.clone(), 0.0, 1.0, &mut rng);
        assert_ne!(varied, line);
        let normalized = normalize_surface_variants(&varied);
        assert_eq!(normalized, line);
    }

    #[test]
    fn keyboard_typo_protects_structure_tokens() {
        let mut rng = ChaCha8Rng::seed_from_u64(19);
        let line = "Scene Event Pair event scene pair".to_string();
        let varied = apply_per_word_keyboard_typos(line.clone(), 1.0, &mut rng);
        assert_eq!(varied, line);
    }

    #[test]
    fn keyboard_neighbor_radius_expands_candidates() {
        let radius_one = keyboard_neighbors_for_char('q', 1);
        assert!(radius_one.contains(&'w'));
        assert!(!radius_one.contains(&'e'));

        let radius_two = keyboard_neighbors_for_char('q', 2);
        assert!(radius_two.contains(&'e'));
    }

    #[test]
    fn keyboard_typo_classes_generate_expected_lengths() {
        let token = "snake";
        let chars = token.chars().collect::<Vec<_>>();
        let mut rng = ChaCha8Rng::seed_from_u64(1234);

        let doubled = apply_keyboard_typo_class(&chars, TypoClass::Doubling, 1, &mut rng)
            .expect("doubling should produce a typo");
        assert_eq!(doubled.chars().count(), chars.len() + 1);

        let omitted = apply_keyboard_typo_class(&chars, TypoClass::Omission, 1, &mut rng)
            .expect("omission should produce a typo");
        assert_eq!(omitted.chars().count(), chars.len() - 1);

        let replaced = apply_keyboard_typo_class(&chars, TypoClass::Replacement, 1, &mut rng)
            .expect("replacement should produce a typo");
        assert_eq!(replaced.chars().count(), chars.len());
        assert_ne!(replaced, token);

        let added_after = apply_keyboard_typo_class(&chars, TypoClass::AdditionAfter, 1, &mut rng)
            .expect("addition-after should produce a typo");
        assert_eq!(added_after.chars().count(), chars.len() + 1);

        let added_before =
            apply_keyboard_typo_class(&chars, TypoClass::AdditionBefore, 1, &mut rng)
                .expect("addition-before should produce a typo");
        assert_eq!(added_before.chars().count(), chars.len() + 1);

        let swapped = apply_keyboard_typo_class(&chars, TypoClass::PairSwap, 1, &mut rng)
            .expect("pair-swap should produce a typo");
        assert_eq!(swapped.chars().count(), chars.len());
        assert_ne!(swapped, token);
    }
}
