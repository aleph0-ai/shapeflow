use maud::{DOCTYPE, Markup, html};

use crate::{
    flow::{self, AnswerKind, PlanItem},
    server::AiNativeInfo,
    stimulus::TaskStimulus,
};

pub fn render_setup_page() -> Markup {
    page_layout(
        "ShapeFlow Human Evaluation",
        html! {
            section #task-panel class="setup-panel" {
                div class="panel-header" style="border-bottom:none;padding-bottom:0" {
                    img class="panel-logo" src="/static/shapeflow.svg" alt="ShapeFlow";
                }
                form method="post" action="/start" class="setup-form" {
                    div class="form-group" {
                        span class="form-label" { "Participant Role" }
                        div class="difficulty-options role-options" {
                            @for (value, lbl) in [("true", "Human"), ("false", "AI Agent")] {
                                label class="difficulty-card" {
                                    input type="radio" name="is_human" id=[if value == "true" { Some("role-human") } else { Some("role-ai") }] value=(value) checked[value == "true"];
                                    div class="difficulty-card-inner" {
                                        span class="difficulty-label" { (lbl) }
                                    }
                                }
                            }
                        }
                    }
                    div class="form-group" id="identifier-group" style="display:none" {
                        label for="identifier-input" { "AI Identifier " small style="font-weight:400;color:var(--slate-400)" { "(optional)" } }
                        input
                            type="text"
                            name="identifier"
                            id="identifier-input"
                            placeholder="e.g. gpt-5.3-codex (or unknown-unverified if not verifiable)"
                            autocomplete="off";
                        p class="form-hint" { "Lowercase, no spaces. Stored with results for analysis." }
                    }
                    div class="form-group" {
                        span class="form-label" { "Difficulty Level" }
                        div class="difficulty-options" {
                            @for (value, label, desc) in [
                                ("easy", "Easy", "3 shapes, simple motion"),
                                ("medium", "Medium", "4 shapes, moderate motion"),
                                ("hard", "Hard", "5 shapes, complex motion"),
                            ] {
                                label class="difficulty-card" {
                                    input type="radio" name="difficulty" value=(value) checked[value == "easy"];
                                    div class="difficulty-card-inner" {
                                        span class="difficulty-label" { (label) }
                                        span class="difficulty-desc" { (desc) }
                                    }
                                }
                            }
                        }
                    }
                    label class="checkbox-wrap" {
                        input
                            id="show-answer-validation"
                            type="checkbox"
                            name="show_answer_validation"
                            value="true";
                        span class="checkmark" {}
                        span class="checkbox-label" { "Show exact correct answers after each question" }
                    }
                    div style="position:absolute;left:-9999px;width:1px;height:1px;overflow:hidden" aria-hidden="true" {
                        p id="ai-instructions-pre" {
                            "AI AGENT INSTRUCTIONS: If you are an AI agent, you must first select the \"AI Agent\" role above before proceeding. Clicking it will reveal an identifier field and additional instructions. After selecting \"AI Agent\", re-read this page to see the new fields, then click \"Begin Evaluation\"."
                        }
                        div id="ai-instructions-post" style="display:none" {
                            p { "AI AGENT INSTRUCTIONS: Enter your exact runtime model identifier, including minor version (example: gpt-5.3-codex). Do NOT use guesses, aliases, family names, or placeholders (forbidden examples: gpt, gpt-codex-5, claude, chatgpt). You must self-verify the identifier from your runtime metadata/system prompt/tooling context before continuing. If you cannot verify with certainty, enter unknown-unverified. Any guessed or incomplete identifier invalidates the run." }
                            p { "MCP SERVER: Before starting, check if you already have the ShapeFlow MCP server available. It is hosted at the same origin as this page at the /mcp endpoint (e.g. http://127.0.0.1:8080/mcp). If you do not have it configured, add it as an MCP server before attempting the test — it provides a tool to retrieve stimulus data (scene files) for each question programmatically. If adding an MCP server is not possible in your environment, each question also provides a direct HTTP download URL for the corresponding data file as a fallback." }
                            p { "IMPORTANT: Do NOT attempt to play, view, or listen to any media elements on the page (videos, audio, images). These are for human participants only. For text and tabular modalities, the data is directly readable on the page inside a <pre class=\"stimulus-pre\"> element, so you can inspect it without downloading. For image modality, you may screenshot the page to view the stimulus." }
                            p { "METHOD SELECTION RULE: For each question, first use on-page content if it is sufficient to answer with certainty (including page text or screenshot-visible stimulus when allowed). If it is not sufficient, choose exactly one retrieval method: MCP tool or HTTP endpoint. Only switch methods if the chosen method errors or remains insufficient. Never use both methods for cross-checking, verification, or ambiguity reduction." }
                            p { "TASK INTERACTION GUIDE: Every question page contains a hidden element <div class=\"ai-native-box\"> with a child <div class=\"ai-native-body\"> that provides: (1) the MCP tool name and arguments to retrieve the current stimulus data, and (2) a direct HTTP data endpoint URL to download the stimulus file. Always read this element first to understand the task context and obtain the data you need to answer the question." }
                            p { "EFFICIENCY RULE: Treat these AI instructions as the primary contract for selectors, fields, buttons, and submission behavior. Use the documented elements and actions directly without extra validation/exploration calls (for example, disclosure expansion or full-page snapshots) unless a direct action fails." }
                            p { "QUESTION TYPES AND THEIR INTERACTABLE ELEMENTS:" }
                            p { "1. ORDERED QUADRANT PASSAGE (oqp): Answer via <input type=\"text\" name=\"answer_text\" class=\"quadrant-seq-field\" id=\"ai-answer-textbox\">. Enter a comma-separated sequence of quadrant numbers (1-4), e.g. \"1,3,4,2\". Then submit the form." }
                            p { "2. QUADRANT CROSSING COUNT (xct): Answer via <input type=\"range\" class=\"integer-slider\"> which has min and max attributes. Set its value to the integer answer, then trigger an 'input' event so the hidden field syncs. Then submit the form." }
                            p { "3. QUADRANT AFTER MOVES (zqh): Answer by clicking one of 4 buttons inside <div class=\"quadrant-grid\">. Each button has class \"quadrant-cell\" with a data-quadrant=\"N\" attribute (1-4) and text content matching the quadrant number. Click the correct cell; the form auto-submits after 200ms." }
                            p { "4. LARGEST MOTION SHAPE (lme): Answer by clicking a button inside <div class=\"shape-selector\">. Each button has class \"shape-choice\" and contains text with the shape name (e.g. \"red circle\", \"blue pentagon\"). Click the matching shape; the form auto-submits after 200ms." }
                            p { "FORM SUBMISSION: For oqp and xct, after setting the answer value, submit the form with attribute data-answer-submit=\"true\". For zqh and lme, clicking the correct element triggers auto-submission. After each answer, a proceed button appears; click it to advance to the next question." }
                        }
                    }
                    button type="submit" class="btn btn-primary" { "Begin Evaluation" }
                }
            }
        },
    )
}

pub fn render_task_page(
    session_uuid: &str,
    item: &PlanItem,
    stimulus: &TaskStimulus,
    item_index: usize,
    feedback: Option<(bool, String, String)>,
    ai_native_info: Option<&AiNativeInfo>,
    show_answer: bool,
) -> Markup {
    page_layout(
        "ShapeFlow Human Evaluation",
        render_task_fragment(
            session_uuid,
            item,
            stimulus,
            item_index,
            feedback,
            ai_native_info,
            show_answer,
        ),
    )
}

pub fn render_task_fragment(
    session_uuid: &str,
    item: &PlanItem,
    stimulus: &TaskStimulus,
    item_index: usize,
    feedback: Option<(bool, String, String)>,
    ai_native_info: Option<&AiNativeInfo>,
    show_answer: bool,
) -> Markup {
    let ai_mode = ai_native_info.is_some();
    let local_index = flow::local_item_index(item_index);
    let task_number = flow::task_number(item_index);

    let total_items = flow::total_items();
    let progress_pct = if total_items > 0 {
        ((item_index as f64 / total_items as f64) * 100.0).round() as u32
    } else {
        0
    };

    let (phase_label, phase_detail) = if item.is_practice {
        (
            "Practice",
            format!(
                "Round {} of {}",
                local_index + 1,
                flow::PRACTICE_SCENES_PER_MODALITY
            ),
        )
    } else {
        (
            "Scored",
            format!(
                "Question {} of {}",
                local_index - flow::PRACTICE_SCENES_PER_MODALITY + 1,
                flow::REAL_SCENES_PER_MODALITY
            ),
        )
    };

    let modality_label = item.modality.as_str();

    let (status_text, status_class) = feedback
        .as_ref()
        .map(|(correct, _, _)| {
            if *correct {
                ("Correct", "feedback-correct")
            } else {
                ("Incorrect", "feedback-incorrect")
            }
        })
        .unwrap_or(("", ""));

    html! {
        section #task-panel class="task-panel" {
            div class="task-header" {
                div class="task-header-top" {
                    div class="task-title-row" {
                        div class="task-number" { "Task " (task_number) }
                        span class="phase-badge" data-phase=(phase_label) { (phase_label) }
                    }
                    div class="modality-badge" { (modality_label) }
                }
                div class="progress-bar-wrap" {
                    div class="progress-bar" style=(format!("width:{}%", progress_pct)) {}
                }
                span class="phase-detail" { (phase_detail) }
            }

            div class="stimulus-area" {
                (render_stimulus(stimulus, ai_mode))
            }

            @if let Some(info) = ai_native_info {
                details class="ai-native-box" {
                    summary class="ai-native-title" { "AI Native Access" }
                    div class="ai-native-body" {
                        p { "Check if you already have the " code { "shapeflow" } " MCP server available to you. If not, ask your user to enable it for you if this option is preferred." }
                        div class="ai-native-detail" {
                            span class="ai-label" { "Tool" }
                            code { "get_eval_sample(seed, difficulty, modality, idx)" }
                        }
                        div class="ai-native-detail" {
                            span class="ai-label" { "Args" }
                            code { (info.tool_args) }
                        }
                        div class="ai-native-divider" {
                            span { "OR" }
                        }
                        div class="ai-native-detail" {
                            span class="ai-label" { "HTTP" }
                            code { (info.data_url) }
                        }
                        p class="ai-native-hint" { "Use the HTTP endpoint to get the current sample as a downloadable file." }
                    }
                }
            }

            div class="question-area" {
                p class="question-prompt" { (item.prompt) }
                @if !matches!(item.answer_kind, AnswerKind::ShapeIdentity | AnswerKind::Quadrant | AnswerKind::QuadrantSequence | AnswerKind::Integer) {
                    p class="answer-hint" { "Format: " (item.answer_hint) }
                }
            }

            @if let Some((is_correct, message, user_answer)) = &feedback {
                @if item.answer_kind == AnswerKind::Quadrant {
                    (render_quadrant_grid_feedback(item, *is_correct, show_answer, user_answer))
                } @else if item.answer_kind == AnswerKind::ShapeIdentity && !item.scene_shapes.is_empty() {
                    (render_shape_selector_feedback(item, *is_correct, show_answer, user_answer))
                } @else {
                    div class=(format!("feedback-banner {}", status_class)) {
                        span class="feedback-icon" {
                            @if status_class == "feedback-correct" { "\u{2714}" } @else { "\u{2718}" }
                        }
                        span class="feedback-text" { (status_text) }
                        @if !message.is_empty() && message != status_text {
                            span class="feedback-detail" { (message) }
                        }
                    }
                }
                form method="post" action="/proceed" data-proceed-submit="true" {
                    input type="hidden" name="session_uuid" value=(session_uuid);
                    @if ai_mode {
                        button
                            type="submit"
                            class="btn btn-primary"
                            id="ai-proceed-next"
                            data-testid="ai-proceed-next" {
                            "Continue"
                        }
                    } @else {
                        button type="submit" class="btn btn-primary" { "Continue" }
                    }
                }
            } @else {
                form method="post" action="/events" data-answer-submit="true" class="answer-form" {
                    input type="hidden" name="session_uuid" value=(session_uuid);
                    input type="hidden" name="question_index" value=(item_index);
                    input type="hidden" name="answer_kind" value=(item.answer_kind.as_str());
                    @if item.answer_kind == AnswerKind::QuadrantSequence {
                        div class="quadrant-seq-input" {
                            div class="quadrant-seq-grid-row" {
                                button type="button" class="btn quadrant-seq-action quadrant-seq-clear" { "Clear" }
                                (render_quadrant_grid_input())
                                button type="button" class="btn quadrant-seq-action quadrant-seq-undo" { "Undo" }
                            }
                            input
                                type="text"
                                name="answer_text"
                                class="quadrant-seq-field"
                                placeholder="Click quadrants above, or type e.g. 1,3,4"
                                data-quadrant-seq="true"
                                id=(if ai_mode { "ai-answer-textbox" } else { "" })
                                data-testid=(if ai_mode { "ai-answer-textbox" } else { "" });
                        }
                    } @else if item.answer_kind == AnswerKind::Quadrant {
                        input type="hidden" name="answer_text" value="" data-quadrant-answer="true";
                        (render_quadrant_grid_input())
                    } @else if item.answer_kind == AnswerKind::Integer {
                        div class="integer-slider-wrap" {
                            input
                                type="range"
                                class="integer-slider"
                                min="0"
                                max=(item.integer_max)
                                value="0"
                                data-integer-slider="true";
                            span class="integer-slider-value" { "0" }
                            input type="hidden" name="answer_text" value="0" data-integer-answer="true";
                        }
                    } @else if item.answer_kind == AnswerKind::ShapeIdentity && !item.scene_shapes.is_empty() {
                        input type="hidden" name="answer_text" value="" data-shape-answer="true";
                        div class="shape-selector" {
                            @for choice in &item.scene_shapes {
                                button
                                    type="button"
                                    class="shape-choice"
                                    data-shape-id=(choice.shape_id) {
                                    (choice.label)
                                }
                            }
                        }
                    } @else {
                        textarea
                            name="answer_text"
                            rows="3"
                            placeholder="Type your answer\u{2026}"
                            id=(if ai_mode { "ai-answer-textbox" } else { "" })
                            data-testid=(if ai_mode { "ai-answer-textbox" } else { "" }) {}
                    }
                    @if matches!(item.answer_kind, AnswerKind::QuadrantSequence | AnswerKind::Integer) || (item.answer_kind == AnswerKind::ShapeIdentity && item.scene_shapes.is_empty()) {
                        @if ai_mode {
                            button
                                type="submit"
                                class="btn btn-primary"
                                id="ai-submit-answer"
                                data-testid="ai-submit-answer" {
                                "Submit Answer"
                            }
                        } @else {
                            button type="submit" class="btn btn-primary" { "Submit Answer" }
                        }
                    }
                }
            }
        }
    }
}

fn render_stimulus(stimulus: &TaskStimulus, ai_mode: bool) -> Markup {
    match stimulus {
        TaskStimulus::Image { data_uri } => html! {
            section class="stimulus-box" {
                img class="stimulus-image" src=(data_uri) alt="Generated image scene";
            }
        },
        TaskStimulus::VideoPlayer {
            frame_data_uris,
            fps,
        } => html! {
            section class="stimulus-box" {
                @if frame_data_uris.is_empty() {
                    p class="stimulus-empty" { "No video frames available for this scene." }
                } @else {
                    div class="video-player" data-fps=(fps) {
                        @for frame_uri in frame_data_uris {
                            input type="hidden" class="video-frame-data" value=(frame_uri);
                        }
                        img
                            class="video-player-frame"
                            src=(frame_data_uris[0])
                            alt="Video stimulus frame";
                        div class="video-player-controls" {
                            button type="button" class="btn btn-small video-toggle" { "\u{25B6} Play" }
                            span class="video-progress" {
                                "Frame 1 / " (frame_data_uris.len())
                            }
                        }
                    }
                }
            }
        },
        TaskStimulus::VideoGif { data_uri } => html! {
            section class="stimulus-box" {
                img class="stimulus-image" src=(data_uri) alt="Generated animated GIF scene";
            }
        },
        TaskStimulus::Text { body } => html! {
            section class="stimulus-box stimulus-text-box" {
                pre class="stimulus-pre" { (body) }
            }
        },
        TaskStimulus::TabularCsv { csv } => {
            if ai_mode {
                html! {
                    section class="stimulus-box stimulus-tabular-box" {
                        pre class="stimulus-pre" { (csv) }
                    }
                }
            } else {
                let table_markup = render_csv_table(csv);
                html! {
                    section class="stimulus-box stimulus-tabular-box" {
                        button type="button" class="tabular-expand-btn" title="Expand" { "\u{26F6}" }
                        div class="stimulus-table-wrap" { (table_markup) }
                    }
                    div class="tabular-overlay" hidden {
                        div class="tabular-overlay-backdrop" {}
                        div class="tabular-overlay-content" {
                            div class="tabular-overlay-header" {
                                span { "Tabular Data" }
                                button type="button" class="tabular-close-btn" { "\u{2715} Close" }
                            }
                            div class="tabular-overlay-table" { (render_csv_table(csv)) }
                        }
                    }
                }
            }
        }
        TaskStimulus::Sound {
            data_uri,
            shape_previews,
            quadrant_guide_data_uri: _,
            transition_previews,
        } => html! {
            section class="stimulus-box stimulus-sound-box" {
                h3 class="stimulus-section-label" { "Shape-to-tone reference" }
                div class="sound-shape-grid" {
                    @for preview in shape_previews {
                        div class="sound-shape-card" {
                            div class="sound-shape-image-wrap" {
                                img class="sound-shape-image" src=(preview.image_data_uri) alt=(preview.label);
                                button
                                    type="button"
                                    class="audio-overlay-button"
                                    data-audio-src=(preview.tone_data_uri) {
                                    "\u{25B6}"
                                }
                            }
                            p class="sound-shape-label" { (preview.label) }
                        }
                    }
                }
                h3 class="stimulus-section-label" { "Quadrant movement examples" }
                div class="sound-quadrant-diagram" {
                    div class="sq-arrow sq-arrow-top" {}
                    div class="sq-arrow sq-arrow-left-side" {}
                    div class="quadrant-grid quadrant-grid-static" {
                        div class="quadrant-cell" { "2" }
                        div class="quadrant-cell" { "1" }
                        div class="quadrant-cell" { "3" }
                        div class="quadrant-cell" { "4" }
                    }
                    div class="sq-arrow sq-arrow-right-side" {}
                    div class="sq-arrow sq-arrow-bottom" {}
                    @if transition_previews.len() > 1 {
                        button type="button" class="sq-play-btn sq-play-top" data-audio-src=(transition_previews[1].audio_data_uri) {}
                    }
                    @if transition_previews.len() > 2 {
                        button type="button" class="sq-play-btn sq-play-left" data-audio-src=(transition_previews[2].audio_data_uri) {}
                    }
                    @if !transition_previews.is_empty() {
                        button type="button" class="sq-play-btn sq-play-right" data-audio-src=(transition_previews[0].audio_data_uri) {}
                    }
                    @if transition_previews.len() > 3 {
                        button type="button" class="sq-play-btn sq-play-bottom" data-audio-src=(transition_previews[3].audio_data_uri) {}
                    }
                }
                h3 class="stimulus-section-label" { "Scene audio" }
                audio controls preload="none" src=(data_uri) {}
            }
        },
    }
}

fn render_csv_table(csv: &str) -> Markup {
    let mut lines = csv.lines();
    let header = lines.next().unwrap_or("");
    let headers: Vec<&str> = header.split(',').collect();
    html! {
        table class="stimulus-table" {
            thead {
                tr {
                    @for h in &headers {
                        th { (h.trim()) }
                    }
                }
            }
            tbody {
                @for line in lines {
                    @if !line.trim().is_empty() {
                        tr {
                            @for cell in line.split(',') {
                                td { (cell.trim()) }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_quadrant_grid_input() -> Markup {
    // Layout: Q2(top-left) Q1(top-right) Q3(bottom-left) Q4(bottom-right)
    html! {
        div class="quadrant-grid" {
            button type="button" class="quadrant-cell" data-quadrant="2" { "2" }
            button type="button" class="quadrant-cell" data-quadrant="1" { "1" }
            button type="button" class="quadrant-cell" data-quadrant="3" { "3" }
            button type="button" class="quadrant-cell" data-quadrant="4" { "4" }
        }
    }
}

fn render_quadrant_grid_feedback(
    item: &PlanItem,
    is_correct: bool,
    show_answer: bool,
    user_answer: &str,
) -> Markup {
    let expected = match &item.expected_answer {
        flow::ExpectedAnswer::Quadrant(q) => Some(*q),
        _ => None,
    };
    let user_pick = user_answer.trim().parse::<usize>().ok();
    let reveal_correct = is_correct || show_answer;
    html! {
        div class="quadrant-grid quadrant-grid-feedback" {
            @for quadrant in [2, 1, 3, 4] {
                @if is_correct && expected == Some(quadrant) {
                    div class="quadrant-cell quadrant-correct" { (quadrant) }
                } @else if !is_correct && user_pick == Some(quadrant) {
                    div class="quadrant-cell quadrant-wrong" { (quadrant) }
                } @else if !is_correct && reveal_correct && expected == Some(quadrant) {
                    div class="quadrant-cell quadrant-correct" { (quadrant) }
                } @else {
                    div class="quadrant-cell quadrant-neutral" { (quadrant) }
                }
            }
        }
        span class=(if is_correct { "inline-feedback inline-feedback-correct" } else { "inline-feedback inline-feedback-incorrect" }) {
            @if is_correct { "\u{2714} Correct" } @else { "\u{2718} Incorrect" }
        }
    }
}

fn render_shape_selector_feedback(
    item: &PlanItem,
    is_correct: bool,
    show_answer: bool,
    user_answer: &str,
) -> Markup {
    let expected_id = match &item.expected_answer {
        flow::ExpectedAnswer::ShapeId(id) => Some(id.as_str()),
        _ => None,
    };
    let user_pick = flow::parse_shape_answer(user_answer).unwrap_or_default();
    let reveal_correct = is_correct || show_answer;
    html! {
        div class="shape-selector shape-selector-feedback" {
            @for choice in &item.scene_shapes {
                @if is_correct && expected_id == Some(choice.shape_id.as_str()) {
                    div class="shape-choice shape-choice-correct" { (choice.label) }
                } @else if !is_correct && choice.shape_id == user_pick {
                    div class="shape-choice shape-choice-wrong" { (choice.label) }
                } @else if !is_correct && reveal_correct && expected_id == Some(choice.shape_id.as_str()) {
                    div class="shape-choice shape-choice-correct" { (choice.label) }
                } @else {
                    div class="shape-choice shape-choice-neutral" { (choice.label) }
                }
            }
            span class=(if is_correct { "inline-feedback inline-feedback-correct" } else { "inline-feedback inline-feedback-incorrect" }) {
                @if is_correct { "\u{2714} Correct" } @else { "\u{2718} Incorrect" }
            }
        }
    }
}

pub fn render_ratings_fragment(session_uuid: &str) -> Markup {
    html! {
        section #task-panel class="task-panel" {
            div class="panel-header" {
                div class="logo-mark" { "SF" }
                h2 { "Session Complete" }
                p class="subtitle" { "One last step \u{2014} rank modality difficulty" }
            }
            div class="ratings-instructions" {
                p { "Assign each modality a unique rank from " strong { "1" } " (easiest) to " strong { "5" } " (hardest)." }
                p class="ratings-note" { "Each value must be used exactly once." }
            }
            form method="post" action="/ratings" data-ratings-submit="true" class="ratings-form" {
                input type="hidden" name="session_uuid" value=(session_uuid);
                div class="ratings-grid" {
                    @for (field, label, icon) in [
                        ("image_difficulty_rating", "Image", "\u{1F5BC}"),
                        ("video_difficulty_rating", "Video", "\u{1F3AC}"),
                        ("text_difficulty_rating", "Text", "\u{1F4DD}"),
                        ("tabular_difficulty_rating", "Tabular", "\u{1F4CA}"),
                        ("sound_difficulty_rating", "Sound", "\u{1F3B5}"),
                    ] {
                        div class="rating-card" {
                            span class="rating-icon" { (icon) }
                            span class="rating-label" { (label) }
                            input
                                min="1" max="5" step="1" required
                                type="number"
                                name=(field)
                                class="rating-input"
                                placeholder="-";
                        }
                    }
                }
                button type="submit" class="btn btn-primary" { "Submit Ratings" }
            }
        }
    }
}

pub fn render_completion_fragment() -> Markup {
    html! {
        section #task-panel class="task-panel completion-panel" {
            div class="completion-content" {
                div class="completion-icon" { "\u{2714}" }
                h2 { "Evaluation Complete" }
                p { "Your answers and difficulty ratings have been recorded." }
                p class="completion-thanks" { "Thank you for participating." }
            }
        }
    }
}

pub fn render_error_fragment(message: &str) -> Markup {
    html! {
        section #task-panel class="task-panel" {
            div class="error-content" {
                div class="error-icon" { "!" }
                h2 { "Something went wrong" }
                p class="error-message" { (message) }
            }
        }
    }
}

fn page_layout(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=DM+Mono:wght@400;500&family=Fraunces:ital,opsz,wght@0,9..144,300..900;1,9..144,300..900&family=Plus+Jakarta+Sans:wght@400;500;600;700&display=swap";
                link rel="stylesheet" href="/static/style.css";
                script defer src="/static/app.js" {}
            }
            body {
                main {
                    (content)
                    footer class="site-footer" {
                        p { "This website is operated by aleph0 s. r. o." }
                        p {
                            a href="https://aleph0.ai/privacy" target="_blank" rel="noopener noreferrer" { "Privacy Policy" }
                            " and "
                            a href="https://aleph0.ai/terms" target="_blank" rel="noopener noreferrer" { "Terms of Use" }
                            " of "
                            a href="https://aleph0.ai/" target="_blank" rel="noopener noreferrer" { "aleph0.ai" }
                            " apply."
                        }
                    }
                }
            }
        }
    }
}

pub fn render_debug_navigator() -> Markup {
    page_layout(
        "ShapeFlow Debug",
        html! {
            section #task-panel class="setup-panel" {
                div class="panel-header" {
                    div class="logo-mark" { "SF" }
                    h2 { "Debug Navigator" }
                    p class="subtitle" { "Preview any stimulus without running a session" }
                }
                form method="get" action="" class="setup-form" id="debug-nav-form" {
                    div class="form-group" {
                        label { "Difficulty" }
                        div class="difficulty-options" {
                            @for (value, label) in [("easy", "Easy"), ("medium", "Medium"), ("hard", "Hard")] {
                                label class="difficulty-card" {
                                    input type="radio" name="difficulty" value=(value) checked[value == "easy"];
                                    div class="difficulty-card-inner" {
                                        span class="difficulty-label" { (label) }
                                    }
                                }
                            }
                        }
                    }
                    div class="form-group" {
                        label { "Modality" }
                        div class="difficulty-options" {
                            @for (value, label) in [("image", "Image"), ("video", "Video"), ("text", "Text"), ("tabular", "Tabular"), ("sound", "Sound")] {
                                label class="difficulty-card" {
                                    input type="radio" name="modality" value=(value) checked[value == "image"];
                                    div class="difficulty-card-inner" {
                                        span class="difficulty-label" { (label) }
                                    }
                                }
                            }
                        }
                    }
                    div class="form-group" {
                        label { "Task" }
                        div class="difficulty-options" style="grid-template-columns: repeat(2, 1fr);" {
                            @for (value, label) in [
                                ("oqp", "Quadrant Passage"),
                                ("xct", "Crossing Count"),
                                ("zqh", "Quadrant After Moves"),
                                ("lme", "Largest Motion Shape"),
                            ] {
                                label class="difficulty-card" {
                                    input type="radio" name="task" value=(value) checked[value == "oqp"];
                                    div class="difficulty-card-inner" {
                                        span class="difficulty-label" { (label) }
                                    }
                                }
                            }
                        }
                    }
                    div class="form-group" {
                        label { "Role" }
                        div class="difficulty-options" {
                            @for (value, label) in [("human", "Human"), ("ai", "AI")] {
                                label class="difficulty-card" {
                                    input type="radio" name="role" value=(value) checked[value == "human"];
                                    div class="difficulty-card-inner" {
                                        span class="difficulty-label" { (label) }
                                    }
                                }
                            }
                        }
                    }
                    button type="submit" class="btn btn-primary" { "Preview Stimulus" }
                }
                script {
                    (maud::PreEscaped(r#"
document.getElementById('debug-nav-form').addEventListener('submit', function(e) {
    e.preventDefault();
    var f = e.target;
    var d = f.elements['difficulty'].value;
    var m = f.elements['modality'].value;
    var t = f.elements['task'].value;
    var r = f.elements['role'].value;
    window.location.href = '/debug/' + d + '/' + m + '/' + t + '/' + r;
});
                    "#))
                }
            }
        },
    )
}
