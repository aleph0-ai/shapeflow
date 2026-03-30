use std::collections::{BTreeMap, BTreeSet};

use rand::RngCore;
use rand_chacha::ChaCha8Rng;

use crate::config::{EasingFamily, SceneConfig, TextReferenceFrame};
use crate::scene_generation::SceneGenerationOutput;
use crate::tabular_encoding::shape_identity_for_index;
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
    pub simultaneous_with: Vec<String>,
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
        let identity = shape_identity_for_index(shape_index)
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

    let mut time_slot_shapes: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    for event in &scene.motion_events {
        if event.shape_index >= shape_count {
            return Err(TextSemanticsError::ShapeIndexOutOfBounds {
                shape_index: event.shape_index,
                shape_count,
            });
        }
        time_slot_shapes
            .entry(event.time_slot)
            .or_default()
            .insert(event.shape_index);
    }

    let mut ordered_events = scene.motion_events.clone();
    ordered_events.sort_by_key(|event| event.global_event_index);

    let mut events = Vec::with_capacity(ordered_events.len());
    for event in &ordered_events {
        let simultaneous_with = time_slot_shapes
            .get(&event.time_slot)
            .expect("time slot should exist for every event")
            .iter()
            .copied()
            .filter(|shape_index| *shape_index != event.shape_index)
            .map(|shape_index| shape_ids[shape_index].clone())
            .collect::<Vec<_>>();
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
    let mut grammar_rng = scene.schedule.text_grammar_rng();
    let mut lexical_rng = scene.schedule.lexical_noise_rng();
    let semantics = derive_scene_text_semantics(scene)?;
    let options = TextSurfaceOptions::from_scene_config(scene_cfg);
    Ok(render_scene_text_lines_from_semantics(
        &semantics,
        TextAlterationProfile::Canonical,
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
    let simultaneous_suffix = if event.simultaneous_with.is_empty() {
        String::new()
    } else {
        format!(
            " while simultaneous with {}",
            event.simultaneous_with.join(", ")
        )
    };
    let relative_suffix = if reference_frame == TextReferenceFrame::Relative {
        " relative to the scene"
    } else {
        ""
    };

    match profile {
        TextAlterationProfile::Canonical | TextAlterationProfile::PairClauseReordered => format!(
            "Event {:04}: the shape ({}) moved from ({:.6}, {:.6}) to ({:.6}, {:.6}){} over {} frames using {}{}.",
            event.event_index,
            event.shape_id,
            event.start_point.x,
            event.start_point.y,
            event.end_point.x,
            event.end_point.y,
            relative_suffix,
            event.duration_frames,
            easing_family_name(event.easing),
            simultaneous_suffix
        ),
        TextAlterationProfile::EventClauseReordered | TextAlterationProfile::FullyReordered => {
            format!(
                "Event {:04}: shape ({}) moved over {} frames using {} from ({:.6}, {:.6}) to ({:.6}, {:.6}){}{}.",
                event.event_index,
                event.shape_id,
                event.duration_frames,
                easing_family_name(event.easing),
                event.start_point.x,
                event.start_point.y,
                event.end_point.x,
                event.end_point.y,
                relative_suffix,
                simultaneous_suffix
            )
        }
    }
}

fn render_pair_line(pair: &PairSemanticFrame, profile: TextAlterationProfile) -> String {
    match profile {
        TextAlterationProfile::Canonical | TextAlterationProfile::EventClauseReordered => format!(
            "Pair {:04}: [event {:04}] {}, {} are {} and {}.",
            pair.pair_index,
            pair.event_index,
            pair.first_shape_id,
            pair.second_shape_id,
            pair.horizontal_relation.as_phrase(),
            pair.vertical_relation.as_phrase()
        ),
        TextAlterationProfile::PairClauseReordered | TextAlterationProfile::FullyReordered => {
            format!(
                "Pair {:04}: [event {:04}] {} is {} and {} relative to {}.",
                pair.pair_index,
                pair.event_index,
                pair.first_shape_id,
                pair.horizontal_relation.as_phrase(),
                pair.vertical_relation.as_phrase(),
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
    let line = if should_apply_from_probability(synonym_rate, lexical_rng) {
        apply_random_replacement(line, &SYNONYM_REPLACEMENTS, lexical_rng)
    } else {
        line
    };

    if should_apply_from_probability(typo_rate, lexical_rng) {
        apply_random_replacement(line, &TYPO_REPLACEMENTS, lexical_rng)
    } else {
        line
    }
}

const SYNONYM_REPLACEMENTS: [(&str, &str); 2] = [
    ("moved", "translated"),
    ("simultaneous with", "concurrent with"),
];

const TYPO_REPLACEMENTS: [(&str, &str); 3] = [
    ("Event ", "Evnet "),
    ("Pair ", "Piar "),
    ("shape (", "shpae ("),
];

fn apply_random_replacement(
    line: String,
    replacements: &[(&str, &str)],
    rng: &mut ChaCha8Rng,
) -> String {
    let candidate_indices = replacements
        .iter()
        .enumerate()
        .filter_map(|(index, (from, _))| line.contains(from).then_some(index))
        .collect::<Vec<_>>();
    if candidate_indices.is_empty() {
        return line;
    }

    let selected = candidate_indices[(rng.next_u32() as usize) % candidate_indices.len()];
    let (from, to) = replacements[selected];
    line.replacen(from, to, 1)
}

fn normalize_surface_variants(line: &str) -> String {
    let mut normalized = line.to_string();
    for (from, to) in [
        ("Evnet ", "Event "),
        ("Piar ", "Pair "),
        ("shpae (", "shape ("),
        ("movde", "moved"),
        ("translated", "moved"),
        ("concurrent with", "simultaneous with"),
    ] {
        normalized = normalized.replace(from, to);
    }
    normalized
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

    let shape_id =
        extract_between(body, "shape (", ")").ok_or_else(|| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;

    let duration_frames = parse_u16_between(body, " over ", " frames using ").ok_or_else(|| {
        TextSemanticsError::ParseLine {
            line: line.to_string(),
        }
    })?;

    let (start_point, end_point) =
        parse_motion_points(body).ok_or_else(|| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;

    let easing_start = body
        .find(" frames using ")
        .map(|index| index + " frames using ".len())
        .ok_or_else(|| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?;
    let easing_tail = &body[easing_start..];
    let easing_end = easing_tail
        .find(" from (")
        .or_else(|| easing_tail.find(" while simultaneous with "))
        .unwrap_or(easing_tail.len());
    let easing_token = easing_tail[..easing_end].trim();
    let easing =
        parse_easing_family(easing_token).ok_or_else(|| TextSemanticsError::ParseEasing {
            token: easing_token.to_string(),
        })?;

    let simultaneous_with = parse_simultaneous_with(body);

    Ok(EventSemanticFrame {
        event_index: u32::try_from(event_index).map_err(|_| TextSemanticsError::ParseLine {
            line: line.to_string(),
        })?,
        shape_id: shape_id.to_string(),
        start_point,
        end_point,
        duration_frames,
        easing,
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

    if let Some((shape_part, relation_part)) = relation_body.split_once(" are ") {
        let (first_shape_id, second_shape_id) =
            shape_part
                .split_once(", ")
                .ok_or_else(|| TextSemanticsError::ParseLine {
                    line: line.to_string(),
                })?;
        let (horizontal_phrase, vertical_phrase) =
            relation_part
                .split_once(" and ")
                .ok_or_else(|| TextSemanticsError::ParseLine {
                    line: line.to_string(),
                })?;
        return Ok(PairSemanticFrame {
            pair_index,
            event_index,
            first_shape_id: first_shape_id.to_string(),
            second_shape_id: second_shape_id.to_string(),
            horizontal_relation: HorizontalSemanticRelation::parse(horizontal_phrase).ok_or_else(
                || TextSemanticsError::ParseRelation {
                    phrase: horizontal_phrase.to_string(),
                },
            )?,
            vertical_relation: VerticalSemanticRelation::parse(vertical_phrase).ok_or_else(
                || TextSemanticsError::ParseRelation {
                    phrase: vertical_phrase.to_string(),
                },
            )?,
        });
    }

    if let Some((subject_part, tail)) = relation_body.split_once(" is ") {
        let (relation_part, object_part) =
            tail.split_once(" relative to ")
                .ok_or_else(|| TextSemanticsError::ParseLine {
                    line: line.to_string(),
                })?;
        let (horizontal_phrase, vertical_phrase) =
            relation_part
                .split_once(" and ")
                .ok_or_else(|| TextSemanticsError::ParseLine {
                    line: line.to_string(),
                })?;
        return Ok(PairSemanticFrame {
            pair_index,
            event_index,
            first_shape_id: subject_part.to_string(),
            second_shape_id: object_part.to_string(),
            horizontal_relation: HorizontalSemanticRelation::parse(horizontal_phrase).ok_or_else(
                || TextSemanticsError::ParseRelation {
                    phrase: horizontal_phrase.to_string(),
                },
            )?,
            vertical_relation: VerticalSemanticRelation::parse(vertical_phrase).ok_or_else(
                || TextSemanticsError::ParseRelation {
                    phrase: vertical_phrase.to_string(),
                },
            )?,
        });
    }

    Err(TextSemanticsError::ParseLine {
        line: line.to_string(),
    })
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

fn extract_between<'a>(text: &'a str, start_marker: &str, end_marker: &str) -> Option<&'a str> {
    let start = text.find(start_marker)? + start_marker.len();
    let tail = &text[start..];
    let end = tail.find(end_marker)?;
    Some(&tail[..end])
}

fn parse_u16_between(text: &str, start_marker: &str, end_marker: &str) -> Option<u16> {
    let value = extract_between(text, start_marker, end_marker)?;
    value.trim().parse::<u16>().ok()
}

fn parse_motion_points(text: &str) -> Option<(NormalizedPoint, NormalizedPoint)> {
    let start_x = text.find("from (")? + "from (".len();
    let tail = &text[start_x..];
    let (start_point_text, tail) = tail.split_once(") to (")?;
    let end_point_text = tail.split(')').next()?;
    let start_point = parse_point(start_point_text)?;
    let end_point = parse_point(end_point_text)?;
    Some((start_point, end_point))
}

fn parse_point(text: &str) -> Option<NormalizedPoint> {
    let (x_text, y_text) = text.split_once(',')?;
    let x = x_text.trim().parse::<f64>().ok()?;
    let y = y_text.trim().parse::<f64>().ok()?;
    NormalizedPoint::new(x, y).ok()
}

fn parse_simultaneous_with(text: &str) -> Vec<String> {
    let Some((_, peers_text)) = text.split_once(" while simultaneous with ") else {
        return Vec::new();
    };
    peers_text
        .trim_end_matches('.')
        .split(", ")
        .filter(|peer| !peer.trim().is_empty())
        .map(|peer| peer.trim().to_string())
        .collect()
}

fn parse_easing_family(token: &str) -> Option<EasingFamily> {
    match token {
        "linear" => Some(EasingFamily::Linear),
        "ease_in" => Some(EasingFamily::EaseIn),
        "ease_out" => Some(EasingFamily::EaseOut),
        "ease_in_out" => Some(EasingFamily::EaseInOut),
        _ => None,
    }
}

fn easing_family_name(easing: EasingFamily) -> &'static str {
    match easing {
        EasingFamily::Linear => "linear",
        EasingFamily::EaseIn => "ease_in",
        EasingFamily::EaseOut => "ease_out",
        EasingFamily::EaseInOut => "ease_in_out",
    }
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
}
