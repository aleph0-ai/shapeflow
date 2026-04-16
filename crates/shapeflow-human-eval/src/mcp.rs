use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::Rng;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, CallToolResult, Content, RawAudioContent, RawContent, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::{db, db::DbPool, flow, flow::Difficulty, stimulus};

const WEB_SESSION_SEED_MIN: u64 = 1u64 << 16;
const WEB_SESSION_SEED_MAX: u64 = (1u64 << 32) - 1;
const SHAPES_PUBLIC_BASE_URL: &str = "https://shapes.aleph0.ai";

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct StartSessionArgs {
    /// Difficulty: easy, medium, or hard.
    pub difficulty: String,
    /// Optional equivalent of the setup-page checkbox.
    pub show_answer_validation: Option<bool>,
    /// Optional run identifier.
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct SessionArgs {
    /// Session UUID returned by start_session.
    pub session_uuid: String,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct SubmitAnswerArgs {
    /// Session UUID returned by start_session.
    pub session_uuid: String,
    /// Answer value in the same format required by answer_kind.
    pub answer_text: String,
    /// Set true only when external scripts/tools were used to derive the answer.
    pub used_tools: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct SubmitDifficultyFeedbackArgs {
    /// Session UUID returned by start_session.
    pub session_uuid: String,
    pub image_difficulty_rating: i16,
    pub video_difficulty_rating: i16,
    pub text_difficulty_rating: i16,
    pub tabular_difficulty_rating: i16,
    pub sound_difficulty_rating: i16,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct GetQuestionSoundReferenceArgs {
    /// Session UUID returned by start_session.
    pub session_uuid: String,
    /// Optional shape id or natural label, defaults to the current query shape or first scene shape.
    pub shape: Option<String>,
}

#[derive(Debug, Serialize)]
struct StartSessionResponse {
    session_uuid: String,
    difficulty: String,
    note: String,
    next_action: String,
}

#[derive(Debug, Serialize)]
struct QuestionResponse {
    session_uuid: String,
    state: String,
    item_index: usize,
    task_number: usize,
    phase_label: String,
    phase_detail: String,
    modality: String,
    target: String,
    answer_kind: String,
    answer_hint: String,
    question: String,
    mcp_data_tool: String,
    mcp_data_args: String,
    data_url: String,
    available_shapes: Option<Vec<String>>,
    sound_reference_tool: Option<String>,
    sound_reference_args: Option<String>,
    sound_reference_url: Option<String>,
    submit_tool: String,
    submit_instruction: String,
    final_feedback_tool: String,
}

#[derive(Debug, Serialize)]
struct SubmitAnswerResponse {
    session_uuid: String,
    item_index: usize,
    is_correct: bool,
    feedback: String,
    next_question_index: usize,
    next_action: String,
}

#[derive(Debug, Serialize)]
struct DifficultyFeedbackResponse {
    session_uuid: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct InstructionsResponse {
    instructions: String,
}

#[derive(Clone)]
pub struct HumanEvalMcpServer {
    tool_router: ToolRouter<Self>,
    pool: Option<DbPool>,
}

#[tool_router]
impl HumanEvalMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            pool: None,
        }
    }

    pub fn new_with_pool(pool: DbPool) -> Self {
        Self {
            tool_router: Self::tool_router(),
            pool: Some(pool),
        }
    }

    #[tool(
        description = "Get mandatory MCP instructions. This should be the first tool call before starting a session."
    )]
    async fn get_instructions(&self) -> Result<CallToolResult, McpError> {
        let payload = InstructionsResponse {
            instructions: [
                "MCP-FIRST REQUIREMENT: This tool is required before starting every session.",
                "Call start_session only after reading these instructions.",
                "For each question, call get_question(session_uuid=...) and then call get_question_data(session_uuid=...) first.",
                "Prefer MCP tool data retrieval. If the MCP response cannot be naturally ingested or read by your agent, do NOT transform or decode it.",
                "Fallback to downloading the question data from https://shapes.aleph0.ai/data/<session_uuid>/<seed>/<difficulty>/<modality>/<index> when MCP data is unusable.",
                "For sound questions, you may call get_question_sound_reference(session_uuid=<uuid>, shape=<shape-id-or-label>) or download https://shapes.aleph0.ai/sound-reference/<session_uuid>/<seed>/<difficulty>/<index>?shape=<shape-id-or-label> for shape-reference audio.",
                "Do not assume model identifier text. If you do not know it exactly, ask the user for the exact model identifier before continuing.",
                "Generic labels are insufficient: \"codex session\", \"claude code\", \"openai\", \"claude\".",
            ]
            .join("\n"),
        };
        Ok(CallToolResult::success(vec![Content::text(json_text(
            &payload,
        ))]))
    }

    #[tool(description = "Create a new AI evaluation session and return session_uuid")]
    async fn start_session(
        &self,
        Parameters(args): Parameters<StartSessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let pool = self.db_pool()?;
        let difficulty = Difficulty::from_str(&args.difficulty)
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let seed = random_web_session_seed();
        let seed_i64 = i64::try_from(seed)
            .map_err(|_| McpError::internal_error("session seed overflow", None))?;
        let session_uuid = Uuid::now_v7().to_string();
        let modality_targets = flow::modality_targets_from_seed(seed);
        let modality_order = flow::modality_order_from_seed(seed);
        let identifier = args
            .identifier
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        db::create_session(
            pool,
            &session_uuid,
            seed_i64,
            difficulty,
            false,
            args.show_answer_validation.unwrap_or(false),
            identifier,
            0,
            &modality_targets,
            &modality_order,
        )
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let payload = StartSessionResponse {
            session_uuid,
            difficulty: difficulty.as_str().to_string(),
            note: "Session started. Before proceeding, ensure you have followed get_instructions (including exact model-id handling). Then follow MCP-first retrieval via get_question(session_uuid=...) and get_question_data(session_uuid=...). If the MCP question payload cannot be naturally ingested/read, use the returned data_url fallback and do not decode or transform it.".to_string(),
            next_action: "get_question(session_uuid=...)".to_string(),
        };
        Ok(CallToolResult::success(vec![Content::text(json_text(
            &payload,
        ))]))
    }

    #[tool(
        description = "Get the current question for a session, including MCP-native data retrieval and submit instructions"
    )]
    async fn get_question(
        &self,
        Parameters(args): Parameters<SessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let pool = self.db_pool()?;
        let session_uuid = normalize_session_uuid(&args.session_uuid)?;
        let state = self
            .load_current_question_state(pool, &session_uuid)
            .await?;
        if state.item_index >= flow::total_items() {
            let payload = QuestionResponse {
                session_uuid: session_uuid.clone(),
                state: "awaiting_difficulty_feedback".to_string(),
                item_index: state.item_index,
                task_number: flow::MODALITY_COUNT,
                phase_label: "Completed".to_string(),
                phase_detail: "All questions answered".to_string(),
                modality: "none".to_string(),
                target: "none".to_string(),
                answer_kind: "none".to_string(),
                answer_hint: String::new(),
                question: "All questions are complete. Submit final difficulty feedback now."
                    .to_string(),
                mcp_data_tool: "get_question_data".to_string(),
                mcp_data_args: format!("session_uuid={session_uuid}"),
                data_url: "not-applicable".to_string(),
                available_shapes: None,
                sound_reference_tool: None,
                sound_reference_args: None,
                sound_reference_url: None,
                submit_tool: "submit_answer".to_string(),
                submit_instruction: "No answer submission is allowed after all questions are complete.".to_string(),
                final_feedback_tool: "submit_difficulty_feedback(session_uuid,image_difficulty_rating,video_difficulty_rating,text_difficulty_rating,tabular_difficulty_rating,sound_difficulty_rating)".to_string(),
            };
            return Ok(CallToolResult::success(vec![Content::text(json_text(
                &payload,
            ))]));
        }

        let item = state.plan_item.ok_or_else(|| {
            McpError::internal_error("missing plan item for active question", None)
        })?;
        let phase_label = if item.is_practice {
            "Practice".to_string()
        } else {
            "Scored".to_string()
        };
        let local_index = flow::local_item_index(item.item_index);
        let phase_detail = if item.is_practice {
            format!(
                "Task {} practice {}/{}",
                flow::task_number(item.item_index),
                local_index + 1,
                flow::PRACTICE_SCENES_PER_MODALITY
            )
        } else {
            format!(
                "Task {} question {}/{}",
                flow::task_number(item.item_index),
                local_index - flow::PRACTICE_SCENES_PER_MODALITY + 1,
                flow::REAL_SCENES_PER_MODALITY
            )
        };
        let submit_instruction = format!(
            "Call submit_answer(session_uuid=\"{}\", answer_text=\"...\", used_tools=false). {}",
            session_uuid,
            answer_instruction_for_kind(item.answer_kind)
        );
        let available_shapes = if item.answer_kind == flow::AnswerKind::ShapeIdentity {
            Some(
                item.scene_shapes
                    .iter()
                    .map(|choice| choice.label.clone())
                    .collect(),
            )
        } else {
            None
        };
        let (sound_reference_tool, sound_reference_args, sound_reference_url) =
            if item.modality == flow::Modality::Sound {
                let sound_shape_id = resolve_sound_shape_for_item(&item, None)?;
                let sound_reference_url = if item.query_shape.is_some() {
                    format!(
                        "{}/sound-reference/{}/{}/{}/{}?shape={}",
                        SHAPES_PUBLIC_BASE_URL,
                        session_uuid,
                        state.seed,
                        state.difficulty.as_str(),
                        item.scene_index,
                        sound_shape_id
                    )
                } else {
                    format!(
                        "{}/sound-reference/{}/{}/{}/{}",
                        SHAPES_PUBLIC_BASE_URL,
                        session_uuid,
                        state.seed,
                        state.difficulty.as_str(),
                        item.scene_index
                    )
                };
                (
                    Some(String::from("get_question_sound_reference")),
                    Some(format!(
                        "session_uuid={session_uuid},shape={sound_shape_id}"
                    )),
                    Some(sound_reference_url),
                )
            } else {
                (None, None, None)
            };

        let payload = QuestionResponse {
            session_uuid: session_uuid.clone(),
            state: "question".to_string(),
            item_index: item.item_index,
            task_number: flow::task_number(item.item_index),
            phase_label,
            phase_detail,
            modality: item.modality.as_str().to_string(),
            target: item.target.as_str().to_string(),
            answer_kind: item.answer_kind.as_str().to_string(),
            answer_hint: item.answer_hint.clone(),
            question: item.prompt.clone(),
            available_shapes,
            mcp_data_tool: "get_question_data".to_string(),
            mcp_data_args: format!("session_uuid={session_uuid}"),
            data_url: format!(
                "{}/data/{session_uuid}/{seed}/{difficulty}/{modality}/{idx}",
                SHAPES_PUBLIC_BASE_URL,
                seed = state.seed,
                difficulty = state.difficulty.as_str(),
                modality = item.modality.as_str(),
                idx = item.scene_index
            ),
            sound_reference_tool,
            sound_reference_args,
            sound_reference_url,
            submit_tool: "submit_answer".to_string(),
            submit_instruction,
            final_feedback_tool: "submit_difficulty_feedback(session_uuid,image_difficulty_rating,video_difficulty_rating,text_difficulty_rating,tabular_difficulty_rating,sound_difficulty_rating)".to_string(),
        };
        Ok(CallToolResult::success(vec![Content::text(json_text(
            &payload,
        ))]))
    }

    #[tool(description = "Fetch data for the current session question and record MCP data usage")]
    async fn get_question_data(
        &self,
        Parameters(args): Parameters<SessionArgs>,
    ) -> Result<CallToolResult, McpError> {
        let pool = self.db_pool()?;
        let session_uuid = normalize_session_uuid(&args.session_uuid)?;
        let state = self
            .load_current_question_state(pool, &session_uuid)
            .await?;
        let item = state.plan_item.ok_or_else(|| {
            McpError::invalid_params(
                "all questions are complete; submit final feedback instead",
                None,
            )
        })?;
        let question_index = question_index_i32(item.item_index)?;
        db::append_used_data_mcp(pool, &session_uuid, question_index)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let payload = stimulus::build_ai_native_sample(
            state.seed,
            state.difficulty,
            item.modality,
            item.scene_index,
        )
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let sample_uri = format!(
            "shapeflow://sample/{session_uuid}/{difficulty}/{modality}/{idx}",
            difficulty = state.difficulty.as_str(),
            modality = item.modality.as_str(),
            idx = item.scene_index
        );

        let result = match payload {
            stimulus::NativeSamplePayload::Text { text, .. } => {
                CallToolResult::success(vec![Content::text(text)])
            }
            stimulus::NativeSamplePayload::Binary { mime_type, bytes } => {
                let blob = STANDARD.encode(&bytes);
                let content = if mime_type == "image/gif" {
                    vec![Content::resource(
                        ResourceContents::blob(blob, sample_uri).with_mime_type(mime_type),
                    )]
                } else if mime_type.starts_with("image/") {
                    vec![Content::image(blob, mime_type)]
                } else if mime_type.starts_with("audio/") {
                    vec![
                        RawContent::Audio(RawAudioContent {
                            data: blob,
                            mime_type,
                        })
                        .no_annotation(),
                    ]
                } else {
                    vec![Content::resource(
                        ResourceContents::blob(blob, sample_uri).with_mime_type(mime_type),
                    )]
                };
                CallToolResult::success(content)
            }
        };
        Ok(result)
    }

    #[tool(
        description = "Fetch shape-specific sound reference for current sound question. Optional shape can be canonical id or natural label."
    )]
    async fn get_question_sound_reference(
        &self,
        Parameters(args): Parameters<GetQuestionSoundReferenceArgs>,
    ) -> Result<CallToolResult, McpError> {
        let pool = self.db_pool()?;
        let session_uuid = normalize_session_uuid(&args.session_uuid)?;
        let state = self
            .load_current_question_state(pool, &session_uuid)
            .await?;
        let item = state.plan_item.ok_or_else(|| {
            McpError::invalid_params(
                "all questions are complete; submit final feedback instead",
                None,
            )
        })?;

        if item.modality != flow::Modality::Sound {
            return Err(McpError::invalid_params(
                "sound references are only available for sound questions",
                None,
            ));
        }

        let shape_id = resolve_sound_shape_for_item(&item, args.shape.as_deref())?;
        let wav = stimulus::build_ai_native_sound_reference(
            state.seed,
            state.difficulty,
            item.scene_index,
            &shape_id,
        )
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let blob = STANDARD.encode(&wav);
        let content = vec![
            RawContent::Audio(RawAudioContent {
                data: blob,
                mime_type: "audio/wav".to_string(),
            })
            .no_annotation(),
        ];
        Ok(CallToolResult::success(content))
    }

    #[tool(description = "Submit answer for current question and advance session to next question")]
    async fn submit_answer(
        &self,
        Parameters(args): Parameters<SubmitAnswerArgs>,
    ) -> Result<CallToolResult, McpError> {
        let pool = self.db_pool()?;
        let session_uuid = normalize_session_uuid(&args.session_uuid)?;
        let state = self
            .load_current_question_state(pool, &session_uuid)
            .await?;
        if state.item_index >= flow::total_items() {
            return Err(McpError::invalid_params(
                "all questions are complete; submit final feedback",
                None,
            ));
        }
        let item = state.plan_item.ok_or_else(|| {
            McpError::internal_error("missing plan item for active question", None)
        })?;
        let (is_correct, feedback) = evaluate_answer(
            &args.answer_text,
            &item.expected_answer,
            state.record.show_answer_validation,
        );
        let next_index = item
            .item_index
            .checked_add(1)
            .ok_or_else(|| McpError::internal_error("task index overflow while advancing", None))?;

        let expected_i32 = i32::try_from(item.item_index)
            .map_err(|_| McpError::internal_error("task index exceeded database limits", None))?;
        let next_i32 = i32::try_from(next_index)
            .map_err(|_| McpError::internal_error("task index exceeded database limits", None))?;
        db::record_answer(
            pool,
            &session_uuid,
            expected_i32,
            next_i32,
            item.modality.as_str(),
            is_correct,
            !item.is_practice,
        )
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        if args.used_tools.unwrap_or(false) {
            let question_index = question_index_i32(item.item_index)?;
            db::append_used_tools(pool, &session_uuid, question_index)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        }

        let payload = SubmitAnswerResponse {
            session_uuid,
            item_index: item.item_index,
            is_correct,
            feedback,
            next_question_index: next_index,
            next_action: if next_index >= flow::total_items() {
                "submit_difficulty_feedback(session_uuid, image_difficulty_rating, video_difficulty_rating, text_difficulty_rating, tabular_difficulty_rating, sound_difficulty_rating)".to_string()
            } else {
                "get_question(session_uuid=...)".to_string()
            },
        };
        Ok(CallToolResult::success(vec![Content::text(json_text(
            &payload,
        ))]))
    }

    #[tool(
        description = "Submit final 1..5 unique difficulty ranking after all questions are completed"
    )]
    async fn submit_difficulty_feedback(
        &self,
        Parameters(args): Parameters<SubmitDifficultyFeedbackArgs>,
    ) -> Result<CallToolResult, McpError> {
        let pool = self.db_pool()?;
        let session_uuid = normalize_session_uuid(&args.session_uuid)?;
        let ratings = [
            args.image_difficulty_rating,
            args.video_difficulty_rating,
            args.text_difficulty_rating,
            args.tabular_difficulty_rating,
            args.sound_difficulty_rating,
        ];
        if !valid_unique_rating_permutation(ratings) {
            return Err(McpError::invalid_params(
                "ratings must use each integer 1 through 5 exactly once (1 easiest, 5 hardest)",
                None,
            ));
        }

        let session = db::get_session(pool, &session_uuid)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
            .ok_or_else(|| McpError::invalid_params("session not found", None))?;
        if session.completed {
            return Err(McpError::invalid_params(
                "session is already completed",
                None,
            ));
        }
        if session.next_question_index < 0
            || (session.next_question_index as usize) < flow::total_items()
        {
            return Err(McpError::invalid_params(
                "please complete all questions before submitting difficulty feedback",
                None,
            ));
        }

        db::store_ratings(
            pool,
            &session_uuid,
            args.image_difficulty_rating,
            args.video_difficulty_rating,
            args.text_difficulty_rating,
            args.tabular_difficulty_rating,
            args.sound_difficulty_rating,
        )
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let payload = DifficultyFeedbackResponse {
            session_uuid,
            status: "completed".to_string(),
        };
        Ok(CallToolResult::success(vec![Content::text(json_text(
            &payload,
        ))]))
    }
}

#[tool_handler]
impl ServerHandler for HumanEvalMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Preferred policy: call get_instructions first, then use start_session -> get_question -> get_question_data -> submit_answer (repeat) -> submit_difficulty_feedback. If MCP question data cannot be naturally ingested/read, use get_question_data's fallback data_url and do not decode or transform it. For sound questions, also use get_question_sound_reference(session_uuid=<uuid>, shape=<shape-id-or-label>) or https://shapes.aleph0.ai/sound-reference/<session_uuid>/<seed>/<difficulty>/<index>?shape=<shape-id-or-label>. Do not assume any model identifier; ask for the exact identifier before continuing."
                    .to_string(),
            )
    }
}

#[derive(Debug, Clone)]
struct CurrentQuestionState {
    record: db::SessionRecord,
    seed: u64,
    difficulty: Difficulty,
    item_index: usize,
    plan_item: Option<flow::PlanItem>,
}

impl HumanEvalMcpServer {
    fn db_pool(&self) -> Result<&DbPool, McpError> {
        self.pool.as_ref().ok_or_else(|| {
            McpError::internal_error("MCP tools require database-backed server context", None)
        })
    }

    async fn load_current_question_state(
        &self,
        pool: &DbPool,
        session_uuid: &str,
    ) -> Result<CurrentQuestionState, McpError> {
        let record = db::get_session(pool, session_uuid)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
            .ok_or_else(|| McpError::invalid_params("session not found", None))?;
        if record.completed {
            return Err(McpError::invalid_params(
                "session is already completed",
                None,
            ));
        }
        if record.next_question_index < 0 {
            return Err(McpError::internal_error(
                "session progress index is invalid",
                None,
            ));
        }
        let item_index = record.next_question_index as usize;
        let seed = u64::try_from(record.seed)
            .map_err(|_| McpError::internal_error("session seed is invalid", None))?;
        let difficulty = db::parse_difficulty(&record.difficulty)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let plan_item = if item_index < flow::total_items() {
            let modality_targets = parse_modality_targets_from_record(&record)?;
            let modality_order = flow::parse_modality_order(&record.modality_order)
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            Some(
                flow::build_plan_item_with_modality_order(
                    seed,
                    difficulty,
                    &modality_targets,
                    &modality_order,
                    item_index,
                )
                .map_err(|error| McpError::internal_error(error.to_string(), None))?,
            )
        } else {
            None
        };

        Ok(CurrentQuestionState {
            record,
            seed,
            difficulty,
            item_index,
            plan_item,
        })
    }
}

fn normalize_session_uuid(raw: &str) -> Result<String, McpError> {
    Uuid::parse_str(raw)
        .map(|value| value.to_string())
        .map_err(|_| McpError::invalid_params("invalid session_uuid", None))
}

fn parse_modality_targets_from_record(
    record: &db::SessionRecord,
) -> Result<flow::ModalityTargets, McpError> {
    let parse_target = |raw: &str| {
        flow::QuestionTarget::from_str(raw)
            .ok_or_else(|| McpError::internal_error(format!("invalid target value '{raw}'"), None))
    };
    Ok([
        parse_target(&record.image_target)?,
        parse_target(&record.video_target)?,
        parse_target(&record.text_target)?,
        parse_target(&record.tabular_target)?,
        parse_target(&record.sound_target)?,
    ])
}

fn random_web_session_seed() -> u64 {
    rand::thread_rng().gen_range(WEB_SESSION_SEED_MIN..=WEB_SESSION_SEED_MAX)
}

fn question_index_i32(item_index: usize) -> Result<i32, McpError> {
    i32::try_from(item_index)
        .map_err(|_| McpError::internal_error("task index exceeded database limits", None))
}

fn answer_instruction_for_kind(kind: flow::AnswerKind) -> &'static str {
    match kind {
        flow::AnswerKind::QuadrantSequence => {
            "For ordered quadrant passage, answer_text must be comma-separated quadrants like \"1,3,4\"."
        }
        flow::AnswerKind::Integer => "For crossing count, answer_text must be an integer.",
        flow::AnswerKind::Quadrant => {
            "For quadrant-after-moves, answer_text must be one quadrant number in 1..4."
        }
        flow::AnswerKind::ShapeIdentity => {
            "For largest-motion-shape, answer_text must be natural shape label like \"red circle\"."
        }
    }
}

fn evaluate_answer(
    raw: &str,
    expected: &flow::ExpectedAnswer,
    show_answer: bool,
) -> (bool, String) {
    match expected {
        flow::ExpectedAnswer::Sequence(target) => {
            let parsed = match flow::parse_sequence(raw) {
                Ok(value) => value,
                Err(error) => {
                    return (
                        false,
                        format!(
                            "Could not parse answer. Use comma-separated quadrants like 1,3,4 ({error})"
                        ),
                    );
                }
            };
            compare_answer(parsed == *target, expected, show_answer)
        }
        flow::ExpectedAnswer::Integer(target) => {
            let parsed = match flow::parse_integer(raw) {
                Ok(value) => value,
                Err(error) => {
                    return (false, format!("Could not parse integer answer ({error})"));
                }
            };
            compare_answer(parsed == *target, expected, show_answer)
        }
        flow::ExpectedAnswer::Quadrant(target) => {
            let parsed = match flow::parse_quadrant(raw) {
                Ok(value) => value,
                Err(error) => {
                    return (false, format!("Could not parse quadrant answer ({error})"));
                }
            };
            compare_answer(parsed == *target, expected, show_answer)
        }
        flow::ExpectedAnswer::ShapeId(target) => {
            let parsed = match flow::parse_shape_answer(raw) {
                Some(value) => value,
                None => {
                    return (
                        false,
                        String::from("Could not parse shape answer (expected e.g. red circle)"),
                    );
                }
            };
            if parsed.is_empty() {
                return (
                    false,
                    String::from("Could not parse shape answer (empty after normalization)"),
                );
            }
            compare_answer(parsed == *target, expected, show_answer)
        }
    }
}

fn compare_answer(
    is_correct: bool,
    expected: &flow::ExpectedAnswer,
    show_answer: bool,
) -> (bool, String) {
    if is_correct {
        return (true, String::new());
    }
    if show_answer {
        (
            false,
            format!("Expected: {}", flow::format_expected_answer(expected)),
        )
    } else {
        (false, "Incorrect".to_string())
    }
}

fn resolve_sound_shape_for_item(
    item: &flow::PlanItem,
    requested_shape: Option<&str>,
) -> Result<String, McpError> {
    if item.scene_shapes.is_empty() {
        return Err(McpError::internal_error(
            "current sound question has no selectable shapes",
            None,
        ));
    }

    let requested_shape_id = if let Some(raw_shape) = requested_shape {
        Some(flow::parse_shape_answer(raw_shape).ok_or_else(|| {
            McpError::invalid_params(
                "shape must be a canonical shape id or natural label (for example red circle)",
                None,
            )
        })?)
    } else {
        None
    };

    let fallback_shape_id = requested_shape_id
        .or_else(|| item.query_shape.clone())
        .or_else(|| {
            item.scene_shapes
                .first()
                .map(|shape| shape.shape_id.clone())
        });

    let fallback_shape_id = fallback_shape_id.ok_or_else(|| {
        McpError::internal_error("current sound question has no selectable shapes", None)
    })?;

    item.scene_shapes
        .iter()
        .find(|shape| shape.shape_id == fallback_shape_id)
        .map(|shape| shape.shape_id.clone())
        .ok_or_else(|| {
            McpError::invalid_params(
                "requested sound shape is not part of the current scene",
                None,
            )
        })
}

fn valid_unique_rating_permutation(values: [i16; 5]) -> bool {
    let mut seen = [false; 5];
    for value in values {
        if !(1..=5).contains(&value) {
            return false;
        }
        let index = (value - 1) as usize;
        if seen[index] {
            return false;
        }
        seen[index] = true;
    }
    true
}

fn json_text<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}
