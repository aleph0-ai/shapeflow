function swapTaskPanel(content) {
    var panel = document.querySelector("#task-panel");
    if (panel) panel.outerHTML = content;
}

function wireInteractiveUi(root) {
    wireAnswerSubmit(root);
    wireProceedSubmit(root);
    wireRatingsSubmit(root);
    wireVideoPlayers(root);
    wireAudioPreviewButtons(root);
    wireShapeSelector(root);
    wireTabularOverlay(root);
    wireQuadrantSelector(root);
    wireQuadrantSequence(root);
    wireIntegerSlider(root);
}

function validateAnswerSyntax(answerKind, answerText) {
    var trimmed = answerText.trim();
    if (!trimmed) return "Answer cannot be empty.";

    if (answerKind === "quadrant_sequence") {
        var parts = trimmed.split(",").filter(function (p) { return p.trim() !== ""; });
        if (parts.length === 0) return "Click quadrants to build the sequence, or type e.g. 1,3,4";
        for (var i = 0; i < parts.length; i++) {
            var token = parts[i].trim();
            if (token.length !== 1 || token < "1" || token > "4")
                return "Each value must be a quadrant: 1, 2, 3, or 4";
        }
        return null;
    }
    if (answerKind === "quadrant") {
        if (trimmed.length !== 1 || trimmed < "1" || trimmed > "4")
            return "Enter a single quadrant integer: 1, 2, 3, or 4.";
        return null;
    }
    if (answerKind === "integer") {
        if (!Number.isInteger(Number(trimmed))) return "Enter an integer value.";
        return null;
    }
    if (answerKind === "shape_identity") {
        if (!/[a-zA-Z]/.test(trimmed))
            return "Enter shape and color text (for example: red circle).";
        return null;
    }
    return null;
}

function wireVideoPlayers(root) {
    root.querySelectorAll(".video-player").forEach(function (player) {
        if (player.dataset.bound === "1") return;
        player.dataset.bound = "1";

        var frameNodes = player.querySelectorAll("input.video-frame-data");
        var frames = Array.from(frameNodes).map(function (n) { return n.value; });
        if (!frames.length) return;

        var image = player.querySelector("img.video-player-frame");
        var toggle = player.querySelector("button.video-toggle");
        var progress = player.querySelector(".video-progress");
        var fpsRaw = Number(player.dataset.fps || "24");
        var fps = Math.max(1, Number.isFinite(fpsRaw) ? fpsRaw : 24);
        var intervalMs = Math.max(16, Math.floor(1000 / fps));
        var frameIndex = 0;
        var timerId = null;

        function renderFrame() {
            image.src = frames[frameIndex];
            progress.textContent = "Frame " + (frameIndex + 1) + " / " + frames.length;
        }

        function stopPlayback() {
            if (timerId !== null) { window.clearInterval(timerId); timerId = null; }
            toggle.textContent = "\u25B6 Play";
        }

        function startPlayback() {
            if (timerId !== null || frames.length < 2) return;
            timerId = window.setInterval(function () {
                frameIndex = (frameIndex + 1) % frames.length;
                renderFrame();
            }, intervalMs);
            toggle.textContent = "\u23F8 Pause";
        }

        toggle.addEventListener("click", function () {
            if (timerId !== null) stopPlayback(); else startPlayback();
        });
        renderFrame();
    });
}

var _defaultVolume = 0.25;

function wireAudioPreviewButtons(root) {
    // Set default volume on all <audio> elements
    root.querySelectorAll("audio[controls]").forEach(function (el) {
        if (el.dataset.volSet !== "1") {
            el.dataset.volSet = "1";
            el.volume = _defaultVolume;
        }
    });

    root.querySelectorAll("button[data-audio-src]").forEach(function (btn) {
        if (btn.dataset.bound === "1") return;
        btn.dataset.bound = "1";
        btn.addEventListener("click", function () {
            var src = btn.getAttribute("data-audio-src");
            if (!src) return;
            var clip = new Audio(src);
            clip.volume = _defaultVolume;
            clip.play().catch(function () {});
        });
    });
}

function wireIntegerSlider(root) {
    var slider = root.querySelector('input[data-integer-slider="true"]');
    if (!slider || slider.dataset.bound === "1") return;
    slider.dataset.bound = "1";
    var display = root.querySelector(".integer-slider-value");
    var hidden = root.querySelector('input[data-integer-answer="true"]');
    if (!display || !hidden) return;
    slider.addEventListener("input", function () {
        display.textContent = slider.value;
        hidden.value = slider.value;
    });
}

function wireTabularOverlay(root) {
    var expandBtn = root.querySelector(".tabular-expand-btn");
    if (!expandBtn || expandBtn.dataset.bound === "1") return;
    expandBtn.dataset.bound = "1";
    var overlay = root.querySelector(".tabular-overlay");
    if (!overlay) return;
    var closeBtn = overlay.querySelector(".tabular-close-btn");
    var backdrop = overlay.querySelector(".tabular-overlay-backdrop");

    expandBtn.addEventListener("click", function () { overlay.hidden = false; });
    if (closeBtn) closeBtn.addEventListener("click", function () { overlay.hidden = true; });
    if (backdrop) backdrop.addEventListener("click", function () { overlay.hidden = true; });
    document.addEventListener("keydown", function (e) {
        if (e.key === "Escape" && !overlay.hidden) overlay.hidden = true;
    });
}

function wireQuadrantSequence(root) {
    var field = root.querySelector('input[data-quadrant-seq="true"]');
    if (!field || field.dataset.bound === "1") return;
    field.dataset.bound = "1";
    var grid = root.querySelector(".quadrant-seq-input .quadrant-grid");
    var clearBtn = root.querySelector(".quadrant-seq-clear");
    var undoBtn = root.querySelector(".quadrant-seq-undo");
    if (!grid) return;

    function scrollToEnd() {
        field.scrollLeft = field.scrollWidth;
    }

    grid.querySelectorAll(".quadrant-cell").forEach(function (cell) {
        cell.addEventListener("click", function () {
            var current = field.value;
            if (current && !current.endsWith(",")) current += ",";
            field.value = current + cell.dataset.quadrant + ",";
            scrollToEnd();
            cell.focus();
        });
    });
    if (clearBtn) clearBtn.addEventListener("click", function () {
        field.value = "";
        clearBtn.focus();
    });
    if (undoBtn) undoBtn.addEventListener("click", function () {
        var v = field.value;
        if (v.endsWith(",")) v = v.slice(0, -1);
        var idx = v.lastIndexOf(",");
        field.value = idx >= 0 ? v.slice(0, idx + 1) : "";
        scrollToEnd();
        undoBtn.focus();
    });
    field.addEventListener("input", scrollToEnd);

    document.addEventListener("keydown", function (e) {
        if (document.activeElement === field) return;
        if (e.key >= "0" && e.key <= "9" || e.key === ",") {
            e.preventDefault();
            field.focus();
            field.value += e.key;
            scrollToEnd();
        }
    });
}

function wireQuadrantSelector(root) {
    var grids = root.querySelectorAll(".quadrant-grid");
    var grid = null;
    for (var i = 0; i < grids.length; i++) {
        if (!grids[i].classList.contains("quadrant-grid-static") && !grids[i].classList.contains("quadrant-grid-feedback")) {
            grid = grids[i]; break;
        }
    }
    if (!grid || grid.dataset.bound === "1") return;
    grid.dataset.bound = "1";
    var hiddenInput = root.querySelector('input[data-quadrant-answer="true"]');
    var form = root.querySelector('form[data-answer-submit="true"]');
    if (!hiddenInput || !form) return;

    grid.querySelectorAll(".quadrant-cell").forEach(function (cell) {
        cell.addEventListener("click", function () {
            hiddenInput.value = cell.dataset.quadrant;
            grid.querySelectorAll(".quadrant-cell").forEach(function (c) {
                c.style.borderColor = "";
                c.style.background = "";
            });
            cell.style.borderColor = "var(--blue-600)";
            cell.style.background = "rgba(37,99,235,0.06)";
            setTimeout(function () { form.requestSubmit(); }, 200);
        });
    });
}

function wireShapeSelector(root) {
    var selector = root.querySelector(".shape-selector");
    if (!selector || selector.dataset.bound === "1") return;
    selector.dataset.bound = "1";
    var hiddenInput = root.querySelector('input[data-shape-answer="true"]');
    var form = root.querySelector('form[data-answer-submit="true"]');
    if (!hiddenInput || !form) return;

    selector.querySelectorAll(".shape-choice").forEach(function (btn) {
        btn.addEventListener("click", function () {
            selector.querySelectorAll(".shape-choice").forEach(function (b) {
                b.classList.remove("selected");
            });
            btn.classList.add("selected");
            var label = btn.textContent.trim();
            hiddenInput.value = label;
            // Auto-submit after a brief moment so user sees selection
            setTimeout(function () { form.requestSubmit(); }, 200);
        });
    });
}

function wireAnswerSubmit(root) {
    var form = root.querySelector('form[data-answer-submit="true"]');
    if (!form || form.dataset.bound === "1") return;
    form.dataset.bound = "1";
    form.addEventListener("submit", async function (event) {
        event.preventDefault();
        var answerText = form.elements["answer_text"].value;
        var answerKind = form.elements["answer_kind"].value;
        var syntaxError = validateAnswerSyntax(answerKind, answerText);
        if (syntaxError) { window.alert(syntaxError); return; }
        var payload = {
            session_uuid: form.elements["session_uuid"].value,
            question_index: Number(form.elements["question_index"].value),
            answer_text: answerText
        };
        var response = await fetch(form.getAttribute("action") || "/events", {
            method: "POST",
            headers: { "Content-Type": "application/json", "Accept": "text/html" },
            body: JSON.stringify(payload)
        });
        var content = await response.text();
        swapTaskPanel(content);
        wireInteractiveUi(document);
        var input = document.querySelector('textarea[name="answer_text"]');
        if (input) input.focus();
    });
    var input = form.querySelector('textarea[name="answer_text"]');
    if (input) input.focus();
}

function wireProceedSubmit(root) {
    var form = root.querySelector('form[data-proceed-submit="true"]');
    if (!form || form.dataset.bound === "1") return;
    form.dataset.bound = "1";
    form.addEventListener("submit", async function (event) {
        event.preventDefault();
        var body = new URLSearchParams(new FormData(form));
        var response = await fetch(form.getAttribute("action") || "/proceed", {
            method: "POST",
            headers: { "Accept": "text/html" },
            body: body
        });
        var content = await response.text();
        swapTaskPanel(content);
        wireInteractiveUi(document);
    });
}

function wireRatingsSubmit(root) {
    var form = root.querySelector('form[data-ratings-submit="true"]');
    if (!form || form.dataset.bound === "1") return;

    function validRatingsPermutation() {
        var ratingFields = [
            "image_difficulty_rating", "video_difficulty_rating",
            "text_difficulty_rating", "tabular_difficulty_rating",
            "sound_difficulty_rating"
        ];
        var values = [];
        for (var i = 0; i < ratingFields.length; i++) {
            var el = form.elements[ratingFields[i]];
            var value = Number(el ? el.value : "");
            if (!Number.isInteger(value) || value < 1 || value > 5) {
                window.alert("Each modality must have an integer rank from 1 to 5.");
                return false;
            }
            values.push(value);
        }
        var unique = new Set(values);
        if (unique.size !== ratingFields.length) {
            window.alert("Use each rank 1 through 5 exactly once (no duplicates).");
            return false;
        }
        for (var rank = 1; rank <= 5; rank++) {
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
        if (!validRatingsPermutation()) return;
        var body = new URLSearchParams(new FormData(form));
        var response = await fetch(form.getAttribute("action") || "/ratings", {
            method: "POST",
            headers: { "Accept": "text/html" },
            body: body
        });
        var content = await response.text();
        swapTaskPanel(content);
        wireInteractiveUi(document);
    });
}

document.addEventListener("DOMContentLoaded", function () {
    wireInteractiveUi(document);

    var identifierGroup = document.getElementById("identifier-group");
    var roleRadios = document.querySelectorAll('input[name="is_human"]');
    var aiPre = document.getElementById("ai-instructions-pre");
    var aiPost = document.getElementById("ai-instructions-post");
    if (roleRadios.length && identifierGroup) {
        function toggleIdentifier() {
            var checked = document.querySelector('input[name="is_human"]:checked');
            var isAi = checked && checked.value === "false";
            identifierGroup.style.display = isAi ? "" : "none";
            if (aiPre) aiPre.style.display = isAi ? "none" : "";
            if (aiPost) aiPost.style.display = isAi ? "" : "none";
        }
        roleRadios.forEach(function (r) { r.addEventListener("change", toggleIdentifier); });
        toggleIdentifier();
    }
});
