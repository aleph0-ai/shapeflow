use maud::{html, Markup, DOCTYPE};
use maud_extensions::{inline_css, inline_js};

use crate::{
    flow::{self, PlanItem},
    stimulus::TaskStimulus,
};

pub fn render_setup_page() -> Markup {
    page_layout(
        "ShapeFlow Human Evaluation",
        html! {
            section #task-panel class="setup-panel" {
                h2 { "Human Evaluation Setup" }
                form method="post" action="/start" {
                    label { "Role" }
                    br;
                    select name="is_human" {
                        option value="true" { "Human" }
                        option value="false" { "AI" }
                    }
                    br; br;
                    label { "Difficulty" }
                    br;
                    select name="difficulty" {
                        option value="easy" { "Easy" }
                        option value="medium" { "Medium" }
                        option value="hard" { "Hard" }
                    }
                    br; br;
                    label {
                        input
                            id="show-answer-validation"
                            type="checkbox"
                            name="show_answer_validation"
                            value="true";
                        span class="tooltip" title="You will always be told if your answer was correct. If checked, you will also see the exact correct answer." { " show exact answer" }
                    }
                    br; br;
                    button type="submit" { "Start session" }
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
    feedback: Option<(bool, String)>,
    ai_native_sample_url: Option<&str>,
) -> Markup {
    page_layout(
        "ShapeFlow Human Evaluation",
        render_task_fragment(
            session_uuid,
            item,
            stimulus,
            item_index,
            feedback,
            ai_native_sample_url,
        ),
    )
}

pub fn render_task_fragment(
    session_uuid: &str,
    item: &PlanItem,
    stimulus: &TaskStimulus,
    item_index: usize,
    feedback: Option<(bool, String)>,
    ai_native_sample_url: Option<&str>,
) -> Markup {
    let ai_mode = ai_native_sample_url.is_some();
    let local_index = flow::local_item_index(item_index);
    let task_number = flow::task_number(item_index);
    let progress_line = if item.is_practice {
        format!(
            "Practice round {} out of {}",
            local_index + 1,
            flow::PRACTICE_SCENES_PER_MODALITY
        )
    } else {
        format!(
            "Question {} / {}",
            local_index - flow::PRACTICE_SCENES_PER_MODALITY + 1,
            flow::REAL_SCENES_PER_MODALITY
        )
    };

    let (status_text, status_class) = feedback
        .as_ref()
        .map(|(correct, _)| {
            if *correct {
                ("Correct", "ok")
            } else {
                ("Incorrect", "bad")
            }
        })
        .unwrap_or(("", ""));

    html! {
        section #task-panel class="task-panel" {
            h2 { "Task " (task_number) }
            p class="progress-line" { (progress_line) }
            (render_stimulus(stimulus))
            @if let Some(tool_args) = ai_native_sample_url {
                section class="stimulus-box ai-native-box" {
                    p class="ai-native-title" { "AI Native Access" }
                    p { "MCP endpoint: /mcp" }
                    p { "Tool: get_eval_sample(seed, difficulty, modality, idx)" }
                    p { "Use these args for the current item:" }
                    code class="ai-native-url" { (tool_args) }
                }
            }
            p { (item.prompt) }
            p { "Answer format: " (item.answer_hint) }
            @if let Some((_, message)) = &feedback {
                p class=(status_class) { (status_text) }
                @if !message.is_empty() && message != status_text {
                    p { (message) }
                }
                form method="post" action="/proceed" data-proceed-submit="true" {
                    input type="hidden" name="session_uuid" value=(session_uuid);
                    @if ai_mode {
                        button
                            type="submit"
                            id="ai-proceed-next"
                            data-testid="ai-proceed-next" {
                            "Proceed"
                        }
                    } @else {
                        button type="submit" { "Proceed" }
                    }
                }
            } @else {
                form method="post" action="/events" data-answer-submit="true" {
                    input type="hidden" name="session_uuid" value=(session_uuid);
                    input type="hidden" name="question_index" value=(item_index);
                    input type="hidden" name="answer_kind" value=(item.answer_kind.as_str());
                    @if ai_mode {
                        textarea
                            name="answer_text"
                            rows="4"
                            cols="36"
                            id="ai-answer-textbox"
                            data-testid="ai-answer-textbox" {}
                    } @else {
                        textarea name="answer_text" rows="4" cols="36" {}
                    }
                    br;
                    @if ai_mode {
                        button
                            type="submit"
                            id="ai-submit-answer"
                            data-testid="ai-submit-answer" {
                            "Submit"
                        }
                    } @else {
                        button type="submit" { "Submit" }
                    }
                }
            }
        }
    }
}

fn render_stimulus(stimulus: &TaskStimulus) -> Markup {
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
                    p { "No video frames available for this scene." }
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
                            button type="button" class="video-toggle" { "Play" }
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
                p { "Animated GIF preview" }
                img class="stimulus-image" src=(data_uri) alt="Generated animated GIF scene";
            }
        },
        TaskStimulus::Text { body } => html! {
            section class="stimulus-box" {
                pre class="stimulus-pre" { (body) }
            }
        },
        TaskStimulus::TabularCsv { csv } => html! {
            section class="stimulus-box" {
                pre class="stimulus-pre" { (csv) }
            }
        },
        TaskStimulus::Sound {
            data_uri,
            shape_previews,
            quadrant_guide_data_uri,
            transition_previews,
        } => html! {
            section class="stimulus-box" {
                p { "Shape-to-tone reference" }
                div class="sound-shape-grid" {
                    @for preview in shape_previews {
                        div class="sound-shape-card" {
                            div class="sound-shape-image-wrap" {
                                img class="sound-shape-image" src=(preview.image_data_uri) alt=(preview.label);
                                button
                                    type="button"
                                    class="audio-overlay-button"
                                    data-audio-src=(preview.tone_data_uri) {
                                    "▶"
                                }
                            }
                            p class="sound-shape-label" { (preview.label) }
                        }
                    }
                }
                p { "Quadrant movement examples (counter-clockwise)" }
                div class="sound-guide-layout" {
                    img class="quadrant-guide-image" src=(quadrant_guide_data_uri) alt="Quadrant map with counter-clockwise arrows";
                    div class="sound-transition-list" {
                        @for transition in transition_previews {
                            button
                                type="button"
                                class="audio-sample-button"
                                data-audio-src=(transition.audio_data_uri) {
                                "Play " (transition.label)
                            }
                        }
                    }
                }
                p { "Scene audio" }
                audio controls preload="none" src=(data_uri) {}
            }
        },
    }
}

pub fn render_ratings_fragment(session_uuid: &str) -> Markup {
    html! {
        section #task-panel class="completion" {
            h2 { "Session complete" }
            p { "Rank modalities from easiest to hardest." }
            p { "Use each value exactly once: 1 is easiest, 5 is hardest (no duplicates)." }
            form method="post" action="/ratings" data-ratings-submit="true" {
                input type="hidden" name="session_uuid" value=(session_uuid);
                table {
                    tr {
                        th { "Modality" }
                        th { "Rank (1..5)" }
                    }
                    @for (field, label) in [
                        ("image_difficulty_rating", "Image"),
                        ("video_difficulty_rating", "Video"),
                        ("text_difficulty_rating", "Text"),
                        ("tabular_difficulty_rating", "Tabular"),
                        ("sound_difficulty_rating", "Sound"),
                    ] {
                        tr {
                            td { (label) }
                            td {
                                input min="1" max="5" step="1" required type="number" name=(field);
                            }
                        }
                    }
                }
                br;
                button type="submit" { "Submit ratings" }
            }
        }
    }
}

pub fn render_completion_fragment() -> Markup {
    html! {
        section #task-panel class="completion" {
            h2 { "Evaluation complete" }
            p { "Your answers and ratings were recorded." }
        }
    }
}

pub fn render_error_fragment(message: &str) -> Markup {
    html! {
        section #task-panel class="completion" {
            h2 { "Request failed" }
            p { (message) }
        }
    }
}

fn page_layout(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { (title) }
                (inline_css! {
                    body {
                        margin: 0;
                        min-height: 100vh;
                        display: grid;
                        place-items: center;
                        background: radial-gradient(circle at 20% 20%, #f7f9ff, #e5ecf5);
                        color: #13253a;
                        font-family: Georgia, "Times New Roman", serif;
                    }
                    main {
                        width: min(760px, calc(100% - 2rem));
                        padding: 1.25rem;
                        border-radius: 14px;
                        border: 1px solid #c5d4ea;
                        background: #ffffffdd;
                        box-shadow: 0 20px 50px #0f203312;
                    }
                    h2, p {
                        margin: 0.25rem 0;
                    }
                    .task-panel, .completion, .setup-panel {
                        display: grid;
                        gap: 0.75rem;
                    }
                    .progress-line {
                        font-weight: 600;
                    }
                    .stimulus-box {
                        border: 1px solid #cedbef;
                        border-radius: 10px;
                        padding: 0.5rem;
                        background: #f7fbff;
                    }
                    .stimulus-image {
                        width: 100%;
                        height: auto;
                        display: block;
                    }
                    .video-player-frame {
                        width: 100%;
                        height: auto;
                        border: 1px solid #c2d6ee;
                        border-radius: 6px;
                    }
                    .video-player-controls {
                        display: flex;
                        align-items: center;
                        gap: 0.5rem;
                        margin-top: 0.4rem;
                    }
                    .stimulus-pre {
                        margin: 0;
                        max-height: 280px;
                        overflow: auto;
                        white-space: pre-wrap;
                    }
                    .ai-native-box {
                        background: #f3f8ff;
                    }
                    .ai-native-title {
                        font-weight: 700;
                    }
                    .ai-native-url {
                        display: block;
                        overflow-wrap: anywhere;
                        font-size: 0.92rem;
                        padding: 0.35rem 0.45rem;
                        border-radius: 6px;
                        border: 1px solid #c2d6ee;
                        background: #ffffff;
                    }
                    .sound-shape-grid {
                        display: grid;
                        grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
                        gap: 0.6rem;
                    }
                    .sound-shape-card {
                        display: grid;
                        gap: 0.35rem;
                    }
                    .sound-shape-image-wrap {
                        position: relative;
                    }
                    .sound-shape-image {
                        width: 100%;
                        display: block;
                    }
                    .audio-overlay-button {
                        position: absolute;
                        inset: 0;
                        border: 0;
                        border-radius: 10px;
                        background: #09101a44;
                        color: #f8fbff;
                        font-size: 1.8rem;
                        font-weight: 700;
                        cursor: pointer;
                    }
                    .audio-overlay-button:hover {
                        background: #09101a66;
                    }
                    .sound-shape-label {
                        margin: 0;
                        text-align: center;
                        font-size: 0.95rem;
                    }
                    .sound-guide-layout {
                        display: grid;
                        grid-template-columns: minmax(0, 1fr) auto;
                        gap: 0.75rem;
                        align-items: start;
                    }
                    .quadrant-guide-image {
                        width: 100%;
                        max-width: 320px;
                        height: auto;
                        border: 1px solid #c2d6ee;
                        border-radius: 8px;
                        background: #ffffff;
                    }
                    .sound-transition-list {
                        display: grid;
                        gap: 0.45rem;
                        align-content: start;
                    }
                    .audio-sample-button {
                        padding: 0.35rem 0.55rem;
                        border: 1px solid #a0b9db;
                        border-radius: 7px;
                        background: #eff6ff;
                        cursor: pointer;
                    }
                    .audio-sample-button:hover {
                        background: #dfeefe;
                    }
                    textarea, input, select, button {
                        font: inherit;
                    }
                    .ok {
                        color: #145b2a;
                        font-weight: 600;
                    }
                    .bad {
                        color: #8b2f2f;
                        font-weight: 600;
                    }
                    .tooltip {
                        cursor: help;
                    }
                    table {
                        border-collapse: collapse;
                        width: 100%;
                    }
                    th, td {
                        padding: 0.2rem 0.4rem;
                        border-bottom: 1px solid #cedbef;
                        text-align: left;
                    }
                })
                (inline_js! {
                    function swapTaskPanel(content) {
                        const panel = document.querySelector("#task-panel");
                        if (panel) {
                            panel.outerHTML = content;
                        }
                    }

                    function wireInteractiveUi(root) {
                        wireAnswerSubmit(root);
                        wireProceedSubmit(root);
                        wireRatingsSubmit(root);
                        wireVideoPlayers(root);
                        wireAudioPreviewButtons(root);
                    }

                    function validateAnswerSyntax(answerKind, answerText) {
                        const trimmed = answerText.trim();
                        if (!trimmed) {
                            return "Answer cannot be empty.";
                        }

                        if (answerKind === "quadrant_sequence") {
                            const parts = trimmed.split(",");
                            if (parts.length === 0) {
                                return "Use comma-separated quadrant integers, for example: 1,3,4";
                            }
                            for (const part of parts) {
                                const token = part.trim();
                                if (token.length !== 1 || token < "1" || token > "4") {
                                    return "Use comma-separated quadrant integers, for example: 1,3,4";
                                }
                            }
                            if (trimmed.endsWith(",")) {
                                return "Use comma-separated quadrant integers, for example: 1,3,4";
                            }
                            return null;
                        }

                        if (answerKind === "quadrant") {
                            if (trimmed.length !== 1 || trimmed < "1" || trimmed > "4") {
                                return "Enter a single quadrant integer: 1, 2, 3, or 4.";
                            }
                            return null;
                        }

                        if (answerKind === "integer") {
                            const parsed = Number(trimmed);
                            if (!Number.isInteger(parsed)) {
                                return "Enter an integer value.";
                            }
                            return null;
                        }

                        if (answerKind === "shape_identity") {
                            if (!/[a-zA-Z]/.test(trimmed)) {
                                return "Enter shape and color text (for example: red circle).";
                            }
                            return null;
                        }

                        return null;
                    }

                    function wireVideoPlayers(root) {
                        const players = root.querySelectorAll(".video-player");
                        players.forEach(function (player) {
                            if (player.dataset.bound === "1") {
                                return;
                            }
                            player.dataset.bound = "1";

                            const frameNodes = player.querySelectorAll("input.video-frame-data");
                            const frames = Array.from(frameNodes).map(function (node) {
                                return node.value;
                            });
                            if (frames.length === 0) {
                                return;
                            }

                            const image = player.querySelector("img.video-player-frame");
                            const toggle = player.querySelector("button.video-toggle");
                            const progress = player.querySelector(".video-progress");
                            const fpsRaw = Number(player.dataset.fps || "24");
                            const fps = Math.max(1, Number.isFinite(fpsRaw) ? fpsRaw : 24);
                            const intervalMs = Math.max(16, Math.floor(1000 / fps));
                            let frameIndex = 0;
                            let timerId = null;

                            function renderFrame() {
                                image.src = frames[frameIndex];
                                progress.textContent = "Frame " + (frameIndex + 1) + " / " + frames.length;
                            }

                            function stopPlayback() {
                                if (timerId !== null) {
                                    window.clearInterval(timerId);
                                    timerId = null;
                                }
                                toggle.textContent = "Play";
                            }

                            function startPlayback() {
                                if (timerId !== null || frames.length < 2) {
                                    return;
                                }
                                timerId = window.setInterval(function () {
                                    frameIndex = (frameIndex + 1) % frames.length;
                                    renderFrame();
                                }, intervalMs);
                                toggle.textContent = "Pause";
                            }

                            toggle.addEventListener("click", function () {
                                if (timerId !== null) {
                                    stopPlayback();
                                } else {
                                    startPlayback();
                                }
                            });

                            renderFrame();
                        });
                    }

                    function wireAudioPreviewButtons(root) {
                        const buttons = root.querySelectorAll("button[data-audio-src]");
                        buttons.forEach(function (button) {
                            if (button.dataset.bound === "1") {
                                return;
                            }
                            button.dataset.bound = "1";
                            button.addEventListener("click", function () {
                                const src = button.getAttribute("data-audio-src");
                                if (!src) {
                                    return;
                                }
                                const clip = new Audio(src);
                                clip.play().catch(function () {});
                            });
                        });
                    }

                    function wireAnswerSubmit(root) {
                        const form = root.querySelector("form[data-answer-submit=\"true\"]");
                        if (!form || form.dataset.bound === "1") {
                            return;
                        }
                        form.dataset.bound = "1";
                        form.addEventListener("submit", async function (event) {
                            event.preventDefault();
                            const answerText = form.elements["answer_text"].value;
                            const answerKind = form.elements["answer_kind"].value;
                            const syntaxError = validateAnswerSyntax(answerKind, answerText);
                            if (syntaxError) {
                                window.alert(syntaxError);
                                return;
                            }
                            const payload = {
                                session_uuid: form.elements["session_uuid"].value,
                                question_index: Number(form.elements["question_index"].value),
                                answer_text: answerText
                            };
                            const response = await fetch(form.getAttribute("action") || "/events", {
                                method: "POST",
                                headers: {
                                    "Content-Type": "application/json",
                                    "Accept": "text/html"
                                },
                                body: JSON.stringify(payload)
                            });
                            const content = await response.text();
                            swapTaskPanel(content);
                            wireInteractiveUi(document);
                            const input = document.querySelector("textarea[name=\"answer_text\"]");
                            if (input) {
                                input.focus();
                            }
                        });
                        const input = form.querySelector("textarea[name=\"answer_text\"]");
                        if (input) {
                            input.focus();
                        }
                    }

                    function wireProceedSubmit(root) {
                        const form = root.querySelector("form[data-proceed-submit=\"true\"]");
                        if (!form || form.dataset.bound === "1") {
                            return;
                        }
                        form.dataset.bound = "1";
                        form.addEventListener("submit", async function (event) {
                            event.preventDefault();
                            const body = new URLSearchParams(new FormData(form));
                            const response = await fetch(form.getAttribute("action") || "/proceed", {
                                method: "POST",
                                headers: {
                                    "Accept": "text/html"
                                },
                                body: body
                            });
                            const content = await response.text();
                            swapTaskPanel(content);
                            wireInteractiveUi(document);
                        });
                    }

                    function wireRatingsSubmit(root) {
                        const form = root.querySelector("form[data-ratings-submit=\"true\"]");
                        if (!form || form.dataset.bound === "1") {
                            return;
                        }

                        function validRatingsPermutation() {
                            const ratingFields = [
                                "image_difficulty_rating",
                                "video_difficulty_rating",
                                "text_difficulty_rating",
                                "tabular_difficulty_rating",
                                "sound_difficulty_rating"
                            ];
                            const values = [];

                            for (const field of ratingFields) {
                                const raw = form.elements[field]?.value;
                                const value = Number(raw);
                                if (!Number.isInteger(value) || value < 1 || value > 5) {
                                    window.alert("Each modality must have an integer rank from 1 to 5.");
                                    return false;
                                }
                                values.push(value);
                            }

                            const unique = new Set(values);
                            if (unique.size !== ratingFields.length) {
                                window.alert("Use each rank 1 through 5 exactly once (no duplicates).");
                                return false;
                            }

                            for (let rank = 1; rank <= 5; rank += 1) {
                                if (!unique.has(rank)) {
                                    window.alert("Use each rank 1 through 5 exactly once (no duplicates).");
                                    return false;
                                }
                            }
                            return true;
                        }

                        form.dataset.bound = "1";
                        form.addEventListener("submit", async function (event) {
                            event.preventDefault();
                            if (!validRatingsPermutation()) {
                                return;
                            }
                            const body = new URLSearchParams(new FormData(form));
                            const response = await fetch(form.getAttribute("action") || "/ratings", {
                                method: "POST",
                                headers: {
                                    "Accept": "text/html"
                                },
                                body: body
                            });
                            const content = await response.text();
                            swapTaskPanel(content);
                            wireInteractiveUi(document);
                        });
                    }

                    document.addEventListener("DOMContentLoaded", function () {
                        wireInteractiveUi(document);
                    });
                })
            }
            body {
                main {
                    (content)
                }
            }
        }
    }
}
