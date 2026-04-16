use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result, anyhow};
use axum::{
    Router,
    extract::{Form, Json, Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rand::Rng;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde::Deserialize;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    HumanEvalServerConfig, db, flow, flow::Difficulty, mcp::HumanEvalMcpServer,
    mcp_session::McpSessionManager, stimulus,
};

const WEB_SESSION_SEED_MIN: u64 = 1u64 << 16;
const WEB_SESSION_SEED_MAX: u64 = (1u64 << 32) - 1;

#[derive(Clone)]
struct AppState {
    pool: db::DbPool,
    sessions: Arc<Mutex<HashMap<Uuid, RuntimeSession>>>,
}

#[derive(Debug, Clone)]
struct CachedTaskArtifacts {
    plan_item: flow::PlanItem,
    stimulus: stimulus::TaskStimulus,
}

#[derive(Debug, Clone)]
struct RuntimeSession {
    seed: u64,
    difficulty: Difficulty,
    is_human: bool,
    show_answer_validation: bool,
    modality_targets: flow::ModalityTargets,
    modality_order: flow::ModalityOrder,
    current_item_index: usize,
    cached_task: Option<CachedTaskArtifacts>,
    awaiting_proceed: bool,
    db_session_id: Option<String>,
    completed: bool,
}

#[derive(Deserialize)]
struct SetupPayload {
    is_human: bool,
    difficulty: String,
    show_answer_validation: Option<bool>,
    identifier: Option<String>,
}

#[derive(Deserialize)]
struct EventPayload {
    session_uuid: Uuid,
    question_index: usize,
    answer_text: String,
    #[serde(default)]
    used_tools: bool,
}

#[derive(Deserialize)]
struct ProceedPayload {
    session_uuid: Uuid,
}

#[derive(Deserialize)]
struct SkipModalityPayload {
    session_uuid: Uuid,
}

#[derive(Deserialize)]
struct RatingsPayload {
    session_uuid: Uuid,
    image_difficulty_rating: i16,
    video_difficulty_rating: i16,
    text_difficulty_rating: i16,
    tabular_difficulty_rating: i16,
    sound_difficulty_rating: i16,
}

#[derive(Deserialize)]
struct SoundReferenceQuery {
    shape: Option<String>,
}

pub async fn run_server(config: HumanEvalServerConfig) -> Result<()> {
    let pool = db::connect_pool(&config.database)
        .await
        .map_err(|error| anyhow!("failed to connect to database: {error}"))?;

    db::ensure_schema(&pool)
        .await
        .map_err(|error| anyhow!("failed to initialize schema: {error}"))?;

    let app_state = AppState {
        pool,
        sessions: Arc::new(Mutex::new(HashMap::new())),
    };
    let mcp_tool_pool = app_state.pool.clone();
    let mcp_service = StreamableHttpService::new(
        move || Ok(HumanEvalMcpServer::new_with_pool(mcp_tool_pool.clone())),
        McpSessionManager::new(&app_state.pool).into(),
        StreamableHttpServerConfig::default(),
    );

    let _ = tracing_subscriber::fmt::try_init();

    let index_handler = if config.debug {
        tracing::info!("debug mode enabled \u{2014} stimulus navigator at /");
        get(debug_index_route)
    } else {
        get(index_route)
    };

    let mut router = Router::new()
        .route("/", index_handler)
        .route("/start", post(start_route))
        .route("/start/:session_uuid", get(resume_route))
        .route("/events", post(submit_route))
        .route("/proceed", post(proceed_route))
        .route("/skip-modality", post(skip_modality_route))
        .route("/ratings", post(ratings_route))
        .route("/static/style.css", get(css_route))
        .route("/static/app.js", get(js_route))
        .route("/static/shapeflow.svg", get(logo_route))
        .route(
            "/sound-reference/:session_uuid/:seed/:difficulty/:index",
            get(sound_reference_route),
        )
        .route(
            "/data/:session_uuid/:seed/:difficulty/:modality/:index",
            get(data_route),
        )
        .route("/favicon.ico", get(favicon_route))
        .route("/healthz", get(health_route))
        .nest_service("/mcp", mcp_service);

    if config.debug {
        router = router.route(
            "/debug/:difficulty/:modality/:task/:role",
            get(debug_preview_route),
        );
    }

    let router = router.with_state(app_state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .map_err(|error| anyhow!("failed to bind {}: {error}", config.bind_addr))?;

    tracing::info!("starting human eval server on {}", config.bind_addr);
    tracing::info!("session setup is available at /");

    axum::serve(listener, router)
        .await
        .map_err(|error| anyhow!("server error: {error}"))
}

async fn index_route() -> Html<String> {
    Html(crate::views::render_setup_page().into_string())
}

async fn start_route(State(state): State<AppState>, Form(payload): Form<SetupPayload>) -> Response {
    let setup = match prepare_setup(&payload) {
        Ok(value) => value,
        Err(error) => {
            return Html(crate::views::render_error_page(&error.to_string()).into_string())
                .into_response();
        }
    };

    let session_uuid = {
        let sessions = state.sessions.lock().await;
        generate_session_uuid(&sessions)
    };

    let modality_targets = flow::modality_targets_from_seed(setup.session_seed);
    let modality_order = flow::modality_order_from_seed(setup.session_seed);
    let session_seed_i64 = match i64::try_from(setup.session_seed) {
        Ok(value) => value,
        Err(_) => {
            return Html(
                crate::views::render_error_page("session seed generation overflow").into_string(),
            )
            .into_response();
        }
    };
    let created = match db::create_session(
        &state.pool,
        &session_uuid.to_string(),
        session_seed_i64,
        setup.difficulty,
        setup.is_human,
        setup.show_answer_validation,
        setup.identifier.as_deref(),
        0,
        &modality_targets,
        &modality_order,
    )
    .await
    {
        Ok(created) => created,
        Err(error) => {
            return Html(crate::views::render_error_page(&error.to_string()).into_string())
                .into_response();
        }
    };

    let mut sessions = state.sessions.lock().await;
    sessions.insert(
        session_uuid,
        RuntimeSession {
            seed: setup.session_seed,
            difficulty: setup.difficulty,
            is_human: setup.is_human,
            show_answer_validation: setup.show_answer_validation,
            modality_targets,
            modality_order,
            current_item_index: match usize::try_from(created.next_question_index) {
                Ok(value) => value,
                Err(_) => {
                    return Html(
                        crate::views::render_error_page("session progress index is invalid")
                            .into_string(),
                    )
                    .into_response();
                }
            },
            cached_task: None,
            awaiting_proceed: false,
            db_session_id: Some(created.session_id),
            completed: false,
        },
    );
    drop(sessions);

    Redirect::to(&format!("/start/{session_uuid}")).into_response()
}

async fn resume_route(
    State(state): State<AppState>,
    Path(session_uuid): Path<Uuid>,
) -> Html<String> {
    {
        let mut sessions = state.sessions.lock().await;
        if let Some(runtime) = sessions.get_mut(&session_uuid) {
            if runtime.completed {
                return Html(crate::views::render_error_page("session not found").into_string());
            }
            return Html(
                render_session_page(&session_uuid, runtime).unwrap_or_else(|error| {
                    crate::views::render_error_page(&error.to_string()).into_string()
                }),
            );
        }
    }

    let Some(record) = (match db::get_session(&state.pool, &session_uuid.to_string()).await {
        Ok(record) => record,
        Err(error) => {
            return Html(crate::views::render_error_page(&error.to_string()).into_string());
        }
    }) else {
        return Html(crate::views::render_error_page("session not found").into_string());
    };

    if record.completed {
        return Html(crate::views::render_error_page("session not found").into_string());
    }

    let difficulty = match db::parse_difficulty(&record.difficulty) {
        Ok(value) => value,
        Err(error) => {
            return Html(crate::views::render_error_page(&error.to_string()).into_string());
        }
    };
    let modality_targets = match parse_modality_targets_from_record(&record) {
        Ok(value) => value,
        Err(error) => {
            return Html(crate::views::render_error_page(&error.to_string()).into_string());
        }
    };
    let modality_order = match parse_modality_order_from_record(&record) {
        Ok(value) => value,
        Err(error) => {
            return Html(crate::views::render_error_page(&error.to_string()).into_string());
        }
    };
    let current_item_index = match usize::try_from(record.next_question_index) {
        Ok(value) => value,
        Err(_) => {
            return Html(
                crate::views::render_error_page("session progress index is invalid").into_string(),
            );
        }
    };
    if current_item_index > flow::total_items() {
        return Html(
            crate::views::render_error_page("session progress index is out of bounds")
                .into_string(),
        );
    }

    let mut runtime = RuntimeSession {
        seed: match u64::try_from(record.seed) {
            Ok(value) => value,
            Err(_) => {
                return Html(
                    crate::views::render_error_page("session seed is invalid").into_string(),
                );
            }
        },
        difficulty,
        is_human: record.is_human,
        show_answer_validation: record.show_answer_validation,
        modality_targets,
        modality_order,
        current_item_index,
        cached_task: None,
        awaiting_proceed: false,
        db_session_id: Some(record.session_id.clone()),
        completed: false,
    };

    let rendered = match render_session_page(&session_uuid, &mut runtime) {
        Ok(html) => html,
        Err(error) => {
            return Html(crate::views::render_error_page(&error.to_string()).into_string());
        }
    };

    let mut sessions = state.sessions.lock().await;
    sessions.insert(session_uuid, runtime);
    drop(sessions);

    Html(rendered)
}

fn parse_modality_targets_from_record(record: &db::SessionRecord) -> Result<flow::ModalityTargets> {
    let parse_target = |raw: &str, column: &str| {
        flow::QuestionTarget::from_str(raw).ok_or_else(|| {
            anyhow!("invalid {column} value '{raw}', expected one of oqp|xct|zqh|lme")
        })
    };

    Ok([
        parse_target(&record.image_target, "image_target")?,
        parse_target(&record.video_target, "video_target")?,
        parse_target(&record.text_target, "text_target")?,
        parse_target(&record.tabular_target, "tabular_target")?,
        parse_target(&record.sound_target, "sound_target")?,
    ])
}

fn parse_modality_order_from_record(record: &db::SessionRecord) -> Result<flow::ModalityOrder> {
    flow::parse_modality_order(&record.modality_order).map_err(|error| {
        anyhow!(
            "invalid modality_order value '{}': {error}",
            record.modality_order
        )
    })
}

fn render_session_page(session_uuid: &Uuid, runtime: &mut RuntimeSession) -> Result<String> {
    if runtime.current_item_index >= flow::total_items() {
        return Ok(crate::views::render_ratings_page(&session_uuid.to_string()).into_string());
    }

    let CachedTaskArtifacts {
        plan_item,
        stimulus,
    } = cache_or_rebuild_task_artifacts(runtime, runtime.current_item_index)?;
    let ai_native_info = if runtime.is_human {
        None
    } else {
        Some(build_ai_native_info(
            &session_uuid.to_string(),
            runtime.seed,
            runtime.difficulty,
            &plan_item,
        ))
    };

    Ok(crate::views::render_task_page(
        &session_uuid.to_string(),
        &plan_item,
        &stimulus,
        runtime.current_item_index,
        None,
        ai_native_info.as_ref(),
        runtime.show_answer_validation,
    )
    .into_string())
}

async fn submit_route(
    State(state): State<AppState>,
    Json(payload): Json<EventPayload>,
) -> Html<String> {
    let mut sessions = state.sessions.lock().await;
    let Some(runtime) = sessions.get_mut(&payload.session_uuid) else {
        return Html(crate::views::render_error_fragment("session not found").into_string());
    };

    if runtime.completed {
        return Html(crate::views::render_completion_fragment().into_string());
    }
    if runtime.awaiting_proceed {
        return Html(
            crate::views::render_error_fragment(
                "please click Proceed before submitting another answer",
            )
            .into_string(),
        );
    }

    let expected_index = runtime.current_item_index;
    if expected_index >= flow::total_items() {
        let session_uuid = payload.session_uuid.to_string();
        return Html(crate::views::render_ratings_fragment(&session_uuid).into_string());
    }

    if payload.question_index != expected_index {
        return Html(
            crate::views::render_error_fragment(
                "task index is stale; please continue from the latest page",
            )
            .into_string(),
        );
    }

    let CachedTaskArtifacts {
        plan_item: expected_plan,
        stimulus: current_stimulus,
    } = match cache_or_rebuild_task_artifacts(runtime, expected_index) {
        Ok(value) => value,
        Err(error) => {
            return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
        }
    };

    let (is_correct, feedback) = evaluate_answer(
        &payload.answer_text,
        &expected_plan.expected_answer,
        runtime.show_answer_validation,
    );

    let next_index = match expected_index.checked_add(1) {
        Some(index) => index,
        None => {
            return Html(
                crate::views::render_error_fragment("task index overflow while advancing")
                    .into_string(),
            );
        }
    };

    if let Some(session_id) = runtime.db_session_id.clone() {
        let payload_index_i32 = match i32::try_from(payload.question_index) {
            Ok(value) => value,
            Err(_) => {
                return Html(
                    crate::views::render_error_fragment("task index exceeded database limits")
                        .into_string(),
                );
            }
        };
        let expected_index_i32 = match i32::try_from(expected_index) {
            Ok(value) => value,
            Err(_) => {
                return Html(
                    crate::views::render_error_fragment("task index exceeded database limits")
                        .into_string(),
                );
            }
        };
        let next_index_i32 = match i32::try_from(next_index) {
            Ok(value) => value,
            Err(_) => {
                return Html(
                    crate::views::render_error_fragment("task index exceeded database limits")
                        .into_string(),
                );
            }
        };

        let db_session = match db::get_session(&state.pool, &session_id).await {
            Ok(Some(value)) => value,
            Ok(None) => {
                return Html(
                    crate::views::render_error_fragment("session not found").into_string(),
                );
            }
            Err(error) => {
                return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
            }
        };
        if db_session.completed {
            runtime.completed = true;
            return Html(crate::views::render_completion_fragment().into_string());
        }
        if db_session.next_question_index != payload_index_i32 {
            return Html(
                crate::views::render_error_fragment(
                    "question index does not match server-side progress",
                )
                .into_string(),
            );
        }

        let updated = match db::record_answer(
            &state.pool,
            &session_id,
            expected_index_i32,
            next_index_i32,
            expected_plan.modality.as_str(),
            is_correct,
            !expected_plan.is_practice,
        )
        .await
        {
            Ok(updated) => updated,
            Err(error) => {
                return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
            }
        };

        runtime.current_item_index = match usize::try_from(updated.next_question_index) {
            Ok(value) => value,
            Err(_) => {
                return Html(
                    crate::views::render_error_fragment("task index exceeded runtime limits")
                        .into_string(),
                );
            }
        };
        if payload.used_tools {
            let question_index = match question_index_i32(expected_index) {
                Ok(value) => value,
                Err(error) => {
                    return Html(
                        crate::views::render_error_fragment(&error.to_string()).into_string(),
                    );
                }
            };
            if let Err(error) =
                db::append_used_tools(&state.pool, &session_id, question_index).await
            {
                return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
            }
        }
    } else {
        runtime.current_item_index = next_index;
    }
    runtime.awaiting_proceed = true;

    let ai_native_info = if runtime.is_human {
        None
    } else {
        Some(build_ai_native_info(
            &payload.session_uuid.to_string(),
            runtime.seed,
            runtime.difficulty,
            &expected_plan,
        ))
    };

    Html(
        crate::views::render_task_fragment(
            &payload.session_uuid.to_string(),
            &expected_plan,
            &current_stimulus,
            expected_index,
            Some((is_correct, feedback, payload.answer_text.clone())),
            ai_native_info.as_ref(),
            runtime.show_answer_validation,
        )
        .into_string(),
    )
}

async fn proceed_route(
    State(state): State<AppState>,
    Form(payload): Form<ProceedPayload>,
) -> Html<String> {
    let mut sessions = state.sessions.lock().await;
    let Some(runtime) = sessions.get_mut(&payload.session_uuid) else {
        return Html(crate::views::render_error_fragment("session not found").into_string());
    };

    if runtime.completed {
        return Html(crate::views::render_completion_fragment().into_string());
    }

    if !runtime.awaiting_proceed {
        return Html(
            crate::views::render_error_fragment(
                "no pending answer confirmation; submit an answer first",
            )
            .into_string(),
        );
    }

    runtime.awaiting_proceed = false;

    let next_index = runtime.current_item_index;
    if next_index >= flow::total_items() {
        let session_uuid = payload.session_uuid.to_string();
        return Html(crate::views::render_ratings_fragment(&session_uuid).into_string());
    }

    let CachedTaskArtifacts {
        plan_item: next_plan,
        stimulus: next_stimulus,
    } = match cache_or_rebuild_task_artifacts(runtime, next_index) {
        Ok(value) => value,
        Err(error) => {
            return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
        }
    };

    let ai_native_info = if runtime.is_human {
        None
    } else {
        Some(build_ai_native_info(
            &payload.session_uuid.to_string(),
            runtime.seed,
            runtime.difficulty,
            &next_plan,
        ))
    };

    Html(
        crate::views::render_task_fragment(
            &payload.session_uuid.to_string(),
            &next_plan,
            &next_stimulus,
            next_index,
            None,
            ai_native_info.as_ref(),
            runtime.show_answer_validation,
        )
        .into_string(),
    )
}

async fn skip_modality_route(
    State(state): State<AppState>,
    Form(payload): Form<SkipModalityPayload>,
) -> Html<String> {
    let mut sessions = state.sessions.lock().await;
    let Some(runtime) = sessions.get_mut(&payload.session_uuid) else {
        return Html(crate::views::render_error_fragment("session not found").into_string());
    };

    if runtime.completed {
        return Html(crate::views::render_completion_fragment().into_string());
    }
    if !runtime.is_human {
        return Html(
            crate::views::render_error_fragment(
                "skip modality is available only for human sessions",
            )
            .into_string(),
        );
    }

    let expected_index = runtime.current_item_index;
    if expected_index >= flow::total_items() {
        let session_uuid = payload.session_uuid.to_string();
        return Html(crate::views::render_ratings_fragment(&session_uuid).into_string());
    }

    let skipped_indices = skipped_indices_for_modality(expected_index);
    let next_index = flow::next_modality_start(expected_index);

    if let Some(session_id) = runtime.db_session_id.clone() {
        let expected_index_i32 = match question_index_i32(expected_index) {
            Ok(value) => value,
            Err(error) => {
                return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
            }
        };

        let db_session = match db::get_session(&state.pool, &session_id).await {
            Ok(Some(value)) => value,
            Ok(None) => {
                return Html(
                    crate::views::render_error_fragment("session not found").into_string(),
                );
            }
            Err(error) => {
                return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
            }
        };
        if db_session.completed {
            runtime.completed = true;
            return Html(crate::views::render_completion_fragment().into_string());
        }
        if db_session.next_question_index < 0 {
            return Html(
                crate::views::render_error_fragment("session progress index is invalid")
                    .into_string(),
            );
        }
        let db_item_index = match usize::try_from(db_session.next_question_index) {
            Ok(value) => value,
            Err(_) => {
                return Html(
                    crate::views::render_error_fragment("task index exceeded runtime limits")
                        .into_string(),
                );
            }
        };
        if db_item_index >= flow::total_items() {
            runtime.current_item_index = db_item_index;
            runtime.awaiting_proceed = false;
            runtime.cached_task = None;
            let session_uuid = payload.session_uuid.to_string();
            return Html(crate::views::render_ratings_fragment(&session_uuid).into_string());
        }
        if db_session.next_question_index != expected_index_i32 {
            return Html(
                crate::views::render_error_fragment(
                    "question index does not match server-side progress",
                )
                .into_string(),
            );
        }

        let skipped_indices_i32 = match skipped_indices
            .iter()
            .copied()
            .map(question_index_i32)
            .collect::<Result<Vec<_>>>()
        {
            Ok(values) => values,
            Err(error) => {
                return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
            }
        };
        if let Err(error) =
            db::append_skipped_questions(&state.pool, &session_id, &skipped_indices_i32).await
        {
            return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
        }

        let next_index_i32 = match question_index_i32(next_index) {
            Ok(value) => value,
            Err(error) => {
                return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
            }
        };
        let updated = match db::advance_session_cursor(
            &state.pool,
            &session_id,
            expected_index_i32,
            next_index_i32,
        )
        .await
        {
            Ok(updated) => updated,
            Err(error) => {
                return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
            }
        };
        runtime.current_item_index = match usize::try_from(updated.next_question_index) {
            Ok(value) => value,
            Err(_) => {
                return Html(
                    crate::views::render_error_fragment("task index exceeded runtime limits")
                        .into_string(),
                );
            }
        };
    } else {
        runtime.current_item_index = next_index;
    }

    runtime.awaiting_proceed = false;
    runtime.cached_task = None;

    let next_index = runtime.current_item_index;
    if next_index >= flow::total_items() {
        let session_uuid = payload.session_uuid.to_string();
        return Html(crate::views::render_ratings_fragment(&session_uuid).into_string());
    }

    let CachedTaskArtifacts {
        plan_item: next_plan,
        stimulus: next_stimulus,
    } = match cache_or_rebuild_task_artifacts(runtime, next_index) {
        Ok(value) => value,
        Err(error) => {
            return Html(crate::views::render_error_fragment(&error.to_string()).into_string());
        }
    };

    Html(
        crate::views::render_task_fragment(
            &payload.session_uuid.to_string(),
            &next_plan,
            &next_stimulus,
            next_index,
            None,
            None,
            runtime.show_answer_validation,
        )
        .into_string(),
    )
}

async fn ratings_route(
    State(state): State<AppState>,
    Form(payload): Form<RatingsPayload>,
) -> Html<String> {
    let ratings = [
        payload.image_difficulty_rating,
        payload.video_difficulty_rating,
        payload.text_difficulty_rating,
        payload.tabular_difficulty_rating,
        payload.sound_difficulty_rating,
    ];
    if !valid_unique_rating_permutation(ratings) {
        return Html(
            crate::views::render_error_fragment(
                "ratings must use each integer 1 through 5 exactly once (1 easiest, 5 hardest)",
            )
            .into_string(),
        );
    }

    let mut sessions = state.sessions.lock().await;
    let Some(runtime) = sessions.get_mut(&payload.session_uuid) else {
        return Html(crate::views::render_error_fragment("session not found").into_string());
    };

    if runtime.completed {
        return Html(crate::views::render_completion_fragment().into_string());
    }

    if runtime.current_item_index < flow::total_items() {
        return Html(
            crate::views::render_error_fragment(
                "please complete all tasks before submitting ratings",
            )
            .into_string(),
        );
    }

    let Some(session_id) = runtime.db_session_id.clone() else {
        runtime.completed = true;
        return Html(crate::views::render_completion_fragment().into_string());
    };

    match db::store_ratings(
        &state.pool,
        &session_id,
        payload.image_difficulty_rating,
        payload.video_difficulty_rating,
        payload.text_difficulty_rating,
        payload.tabular_difficulty_rating,
        payload.sound_difficulty_rating,
    )
    .await
    {
        Ok(_) => {
            runtime.completed = true;
            Html(crate::views::render_completion_fragment().into_string())
        }
        Err(error) => Html(crate::views::render_error_fragment(&error.to_string()).into_string()),
    }
}

async fn css_route() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/style.css")),
    )
        .into_response()
}

async fn js_route() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/app.js")),
    )
        .into_response()
}

async fn logo_route() -> Response {
    (
        [(header::CONTENT_TYPE, "image/svg+xml")],
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/shapeflow.svg")),
    )
        .into_response()
}

async fn data_route(
    State(state): State<AppState>,
    Path((session_uuid, seed, difficulty, modality, index)): Path<(
        String,
        u64,
        String,
        String,
        u32,
    )>,
) -> Response {
    let requested_difficulty = match Difficulty::from_str(&difficulty) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
        }
    };

    let requested_modality = match flow::Modality::from_str(&modality.to_ascii_lowercase()) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "invalid modality; expected image|video|text|tabular|sound",
            )
                .into_response();
        }
    };

    let session = match db::get_session(&state.pool, &session_uuid).await {
        Ok(Some(record)) => record,
        Ok(None) => return (StatusCode::NOT_FOUND, "session not found").into_response(),
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    if session.completed {
        return (StatusCode::FORBIDDEN, "session is already completed").into_response();
    }
    if session.next_question_index < 0 {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "session progress index is invalid",
        )
            .into_response();
    }

    let item_index = session.next_question_index as usize;
    if item_index >= flow::total_items() {
        return (StatusCode::FORBIDDEN, "all questions are complete").into_response();
    }

    let session_seed = match u64::try_from(session.seed) {
        Ok(value) => value,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "session seed is invalid").into_response();
        }
    };
    let session_difficulty = match db::parse_difficulty(&session.difficulty) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let modality_targets = match parse_modality_targets_from_record(&session) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let modality_order = match parse_modality_order_from_record(&session) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let current_item = match flow::build_plan_item_with_modality_order(
        session_seed,
        session_difficulty,
        &modality_targets,
        &modality_order,
        item_index,
    ) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };

    if seed != session_seed
        || requested_difficulty != session_difficulty
        || requested_modality != current_item.modality
        || index != current_item.scene_index
    {
        return (
            StatusCode::FORBIDDEN,
            "requested data does not match the session's current question",
        )
            .into_response();
    }

    let question_index = match question_index_i32(item_index) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    if let Err(error) = db::append_used_data_route(&state.pool, &session_uuid, question_index).await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
    }

    let payload = match stimulus::build_ai_native_sample(
        session_seed,
        session_difficulty,
        requested_modality,
        index,
    ) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };

    match payload {
        stimulus::NativeSamplePayload::Text { mime_type, text } => {
            ([(header::CONTENT_TYPE, mime_type)], text).into_response()
        }
        stimulus::NativeSamplePayload::Binary { mime_type, bytes } => {
            ([(header::CONTENT_TYPE, mime_type)], bytes).into_response()
        }
    }
}

async fn sound_reference_route(
    State(state): State<AppState>,
    Path((session_uuid, seed, difficulty, index)): Path<(String, u64, String, u32)>,
    Query(query): Query<SoundReferenceQuery>,
) -> Response {
    let requested_difficulty = match Difficulty::from_str(&difficulty) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
        }
    };

    let session = match db::get_session(&state.pool, &session_uuid).await {
        Ok(Some(record)) => record,
        Ok(None) => return (StatusCode::NOT_FOUND, "session not found").into_response(),
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    if session.completed {
        return (StatusCode::FORBIDDEN, "session is already completed").into_response();
    }
    if session.next_question_index < 0 {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "session progress index is invalid",
        )
            .into_response();
    }

    let item_index = session.next_question_index as usize;
    if item_index >= flow::total_items() {
        return (StatusCode::FORBIDDEN, "all questions are complete").into_response();
    }

    let session_seed = match u64::try_from(session.seed) {
        Ok(value) => value,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "session seed is invalid").into_response();
        }
    };
    let session_difficulty = match db::parse_difficulty(&session.difficulty) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let modality_targets = match parse_modality_targets_from_record(&session) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let modality_order = match parse_modality_order_from_record(&session) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let current_item = match flow::build_plan_item_with_modality_order(
        session_seed,
        session_difficulty,
        &modality_targets,
        &modality_order,
        item_index,
    ) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };

    if seed != session_seed
        || requested_difficulty != session_difficulty
        || index != current_item.scene_index
        || current_item.modality != flow::Modality::Sound
    {
        return (
            StatusCode::FORBIDDEN,
            "requested data does not match the session's current question",
        )
            .into_response();
    }

    let question_index = match question_index_i32(item_index) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    if let Err(error) = db::append_used_data_route(&state.pool, &session_uuid, question_index).await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
    }

    let shape_id = match resolve_sound_shape_for_item(&current_item, query.shape.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            return match error {
                ShapeReferenceResolutionError::BadRequest(message) => {
                    (StatusCode::BAD_REQUEST, message).into_response()
                }
                ShapeReferenceResolutionError::InternalServer(message) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
                }
            };
        }
    };

    let bytes = match stimulus::build_ai_native_sound_reference(
        session_seed,
        session_difficulty,
        index,
        &shape_id,
    ) {
        Ok(value) => value,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    ([(header::CONTENT_TYPE, "audio/wav")], bytes).into_response()
}

async fn debug_index_route() -> Html<String> {
    Html(crate::views::render_debug_navigator().into_string())
}

async fn debug_preview_route(
    State(state): State<AppState>,
    Path((difficulty, modality, task, role)): Path<(String, String, String, String)>,
) -> Response {
    let difficulty = match Difficulty::from_str(&difficulty) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let modality = match flow::Modality::from_str(&modality.to_ascii_lowercase()) {
        Some(v) => v,
        None => {
            return (StatusCode::BAD_REQUEST, "invalid modality").into_response();
        }
    };
    let target = match flow::QuestionTarget::from_str(&task) {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "invalid task; expected oqp|xct|zqh|lme",
            )
                .into_response();
        }
    };
    let is_human = role != "ai";
    let seed = random_web_session_seed();

    let modality_index = flow::modality_order_index(modality);
    let item_index =
        modality_index * flow::SCENES_PER_MODALITY_TOTAL + flow::PRACTICE_SCENES_PER_MODALITY;

    let mut modality_targets = flow::modality_targets_from_seed(seed);
    modality_targets[modality_index] = target;
    let item = match flow::build_plan_item(seed, difficulty, &modality_targets, item_index) {
        Ok(item) => item,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let stim = match stimulus::build_task_stimulus(seed, difficulty, &item, is_human) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let cached_task = CachedTaskArtifacts {
        plan_item: item.clone(),
        stimulus: stim.clone(),
    };

    let session_uuid = Uuid::now_v7();
    let mut sessions = state.sessions.lock().await;
    sessions.insert(
        session_uuid,
        RuntimeSession {
            seed,
            difficulty,
            is_human,
            show_answer_validation: true,
            modality_targets,
            modality_order: flow::canonical_modality_order(),
            current_item_index: item_index,
            cached_task: Some(cached_task),
            awaiting_proceed: false,
            db_session_id: None,
            completed: false,
        },
    );
    drop(sessions);

    let ai_native_info = if is_human {
        None
    } else {
        Some(build_ai_native_info(
            &session_uuid.to_string(),
            seed,
            difficulty,
            &item,
        ))
    };

    Html(
        crate::views::render_task_page(
            &session_uuid.to_string(),
            &item,
            &stim,
            item_index,
            None,
            ai_native_info.as_ref(),
            true,
        )
        .into_string(),
    )
    .into_response()
}

async fn health_route() -> &'static str {
    "ok"
}

async fn favicon_route() -> StatusCode {
    StatusCode::NO_CONTENT
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
                            "Could not parse answer. Use format: comma-separated quadrants like 1,3,4 ({error})"
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
        (false, String::from("Incorrect"))
    }
}

fn prepare_setup(payload: &SetupPayload) -> Result<SetupPayloadState> {
    let difficulty = Difficulty::from_str(&payload.difficulty)?;
    let session_seed = random_web_session_seed();

    let identifier = payload
        .identifier
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(String::from);

    Ok(SetupPayloadState {
        difficulty,
        is_human: payload.is_human,
        show_answer_validation: payload.show_answer_validation.unwrap_or(false),
        identifier,
        session_seed,
    })
}

fn random_web_session_seed() -> u64 {
    rand::thread_rng().gen_range(WEB_SESSION_SEED_MIN..=WEB_SESSION_SEED_MAX)
}

fn generate_session_uuid(sessions: &HashMap<Uuid, RuntimeSession>) -> Uuid {
    loop {
        let session_uuid = Uuid::now_v7();
        if !sessions.contains_key(&session_uuid) {
            return session_uuid;
        }
    }
}

fn cache_or_rebuild_task_artifacts(
    runtime: &mut RuntimeSession,
    item_index: usize,
) -> Result<CachedTaskArtifacts> {
    if let Some(cached) = runtime.cached_task.as_ref()
        && cached.plan_item.item_index == item_index
    {
        return Ok(cached.clone());
    }

    let plan_item = flow::build_plan_item_with_modality_order(
        runtime.seed,
        runtime.difficulty,
        &runtime.modality_targets,
        &runtime.modality_order,
        item_index,
    )
    .map_err(|error| {
        anyhow!("failed to rebuild plan item for session item_index={item_index}: {error}")
    })?;
    let stimulus = stimulus::build_task_stimulus(
        runtime.seed,
        runtime.difficulty,
        &plan_item,
        runtime.is_human,
    )
    .map_err(|error| {
        anyhow!("failed to rebuild stimulus for session item_index={item_index}: {error}")
    })?;

    let cached = CachedTaskArtifacts {
        plan_item: plan_item.clone(),
        stimulus: stimulus.clone(),
    };
    runtime.cached_task = Some(cached.clone());
    Ok(cached)
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

fn build_ai_native_info(
    session_uuid: &str,
    seed: u64,
    difficulty: Difficulty,
    item: &flow::PlanItem,
) -> AiNativeInfo {
    AiNativeInfo {
        tool_args: format!("session_uuid={session_uuid}"),
        data_url: format!(
            "/data/{session_uuid}/{seed}/{difficulty}/{modality}/{idx}",
            session_uuid = session_uuid,
            seed = seed,
            difficulty = difficulty.as_str(),
            modality = item.modality.as_str(),
            idx = item.scene_index,
        ),
    }
}

#[derive(Debug, Clone)]
pub struct AiNativeInfo {
    pub tool_args: String,
    pub data_url: String,
}

#[derive(Debug, Clone)]
struct SetupPayloadState {
    difficulty: Difficulty,
    is_human: bool,
    show_answer_validation: bool,
    identifier: Option<String>,
    session_seed: u64,
}

fn question_index_i32(item_index: usize) -> Result<i32> {
    i32::try_from(item_index).context("task index exceeded database limits")
}

fn skipped_indices_for_modality(item_index: usize) -> Vec<usize> {
    let (_, block_end) = flow::modality_block_bounds(item_index);
    (item_index..block_end).collect()
}

enum ShapeReferenceResolutionError {
    BadRequest(String),
    InternalServer(String),
}

fn resolve_sound_shape_for_item(
    item: &flow::PlanItem,
    requested_shape: Option<&str>,
) -> Result<String, ShapeReferenceResolutionError> {
    if item.scene_shapes.is_empty() {
        return Err(ShapeReferenceResolutionError::InternalServer(
            "current sound question has no selectable shapes".to_string(),
        ));
    }

    let requested_shape_id = if let Some(raw_shape) = requested_shape {
        Some(flow::parse_shape_answer(raw_shape).ok_or_else(|| {
            ShapeReferenceResolutionError::BadRequest(
                "shape must be a canonical shape id or natural label (for example red circle)"
                    .to_string(),
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

    let fallback_shape_id = match fallback_shape_id {
        Some(value) => value,
        None => {
            return Err(ShapeReferenceResolutionError::InternalServer(
                "current sound question has no selectable shapes".to_string(),
            ));
        }
    };

    item.scene_shapes
        .iter()
        .find(|shape| shape.shape_id == fallback_shape_id)
        .map(|shape| shape.shape_id.clone())
        .ok_or_else(|| {
            ShapeReferenceResolutionError::BadRequest(
                "requested sound shape is not part of the current scene".to_string(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_setup_generates_reserved_web_seed_range() {
        for _ in 0..1024 {
            let payload = SetupPayload {
                is_human: false,
                difficulty: "easy".to_string(),
                show_answer_validation: None,
                identifier: None,
            };
            let setup = prepare_setup(&payload).expect("setup should be valid");
            assert!((WEB_SESSION_SEED_MIN..=WEB_SESSION_SEED_MAX).contains(&setup.session_seed));
        }
    }

    #[test]
    fn valid_unique_rating_permutation_accepts_ranked_scale_once_each() {
        assert!(valid_unique_rating_permutation([1, 2, 3, 4, 5]));
        assert!(valid_unique_rating_permutation([5, 4, 3, 2, 1]));
    }

    #[test]
    fn valid_unique_rating_permutation_rejects_duplicates_or_out_of_range() {
        assert!(!valid_unique_rating_permutation([1, 1, 3, 4, 5]));
        assert!(!valid_unique_rating_permutation([0, 2, 3, 4, 5]));
        assert!(!valid_unique_rating_permutation([1, 2, 3, 4, 6]));
    }

    #[test]
    fn skipped_indices_for_modality_match_remaining_block_range() {
        assert_eq!(skipped_indices_for_modality(3), vec![3, 4, 5, 6]);
        assert_eq!(
            flow::next_modality_start(3),
            flow::SCENES_PER_MODALITY_TOTAL
        );
        assert_eq!(
            skipped_indices_for_modality(flow::total_items() - 1),
            vec![34]
        );
    }

    #[test]
    fn cached_task_artifacts_rebuilds_when_missing_or_stale() {
        let mut runtime = RuntimeSession {
            seed: 1337,
            difficulty: Difficulty::Easy,
            is_human: true,
            show_answer_validation: false,
            modality_targets: [
                flow::QuestionTarget::OrderedQuadrantPassage,
                flow::QuestionTarget::CrossingCount,
                flow::QuestionTarget::QuadrantAfterMoves,
                flow::QuestionTarget::LargestMotionShape,
                flow::QuestionTarget::OrderedQuadrantPassage,
            ],
            modality_order: flow::canonical_modality_order(),
            current_item_index: 0,
            cached_task: None,
            awaiting_proceed: false,
            db_session_id: None,
            completed: false,
        };

        let first = cache_or_rebuild_task_artifacts(&mut runtime, 0)
            .expect("first cache rebuild should succeed");
        assert_eq!(first.plan_item.item_index, 0);
        assert_eq!(
            runtime
                .cached_task
                .as_ref()
                .expect("cache should be populated")
                .plan_item
                .item_index,
            0
        );

        let second =
            cache_or_rebuild_task_artifacts(&mut runtime, 0).expect("cache hit should succeed");
        assert_eq!(second.plan_item.item_index, 0);
        assert_eq!(second.plan_item.scene_index, first.plan_item.scene_index);

        runtime.current_item_index = 3;
        let next_index = runtime.current_item_index;
        let third = cache_or_rebuild_task_artifacts(&mut runtime, next_index)
            .expect("stale cache should be rebuilt");
        assert_eq!(third.plan_item.item_index, 3);
        assert_eq!(
            runtime
                .cached_task
                .as_ref()
                .expect("cache should be updated")
                .plan_item
                .item_index,
            3
        );
    }

    #[test]
    fn parse_modality_targets_from_record_decodes_all_target_columns() {
        let record = db::SessionRecord {
            session_id: "s".to_string(),
            seed: 42,
            difficulty: "easy".to_string(),
            is_human: true,
            show_answer_validation: false,
            next_question_index: 0,
            image_target: "oqp".to_string(),
            video_target: "xct".to_string(),
            text_target: "zqh".to_string(),
            tabular_target: "lme".to_string(),
            sound_target: "oqp".to_string(),
            modality_order: "0,1,2,3,4".to_string(),
            skipped_questions: Vec::new(),
            completed: false,
        };

        let targets =
            parse_modality_targets_from_record(&record).expect("targets should parse cleanly");
        assert_eq!(targets[0], flow::QuestionTarget::OrderedQuadrantPassage);
        assert_eq!(targets[1], flow::QuestionTarget::CrossingCount);
        assert_eq!(targets[2], flow::QuestionTarget::QuadrantAfterMoves);
        assert_eq!(targets[3], flow::QuestionTarget::LargestMotionShape);
        assert_eq!(targets[4], flow::QuestionTarget::OrderedQuadrantPassage);
    }

    #[test]
    fn render_session_page_shows_ratings_when_questions_are_complete() {
        let mut runtime = RuntimeSession {
            seed: 1337,
            difficulty: Difficulty::Easy,
            is_human: true,
            show_answer_validation: false,
            modality_targets: [
                flow::QuestionTarget::OrderedQuadrantPassage,
                flow::QuestionTarget::CrossingCount,
                flow::QuestionTarget::QuadrantAfterMoves,
                flow::QuestionTarget::LargestMotionShape,
                flow::QuestionTarget::OrderedQuadrantPassage,
            ],
            modality_order: flow::canonical_modality_order(),
            current_item_index: flow::total_items(),
            cached_task: None,
            awaiting_proceed: false,
            db_session_id: Some("abc".to_string()),
            completed: false,
        };

        let html = render_session_page(&Uuid::now_v7(), &mut runtime)
            .expect("rendering ratings state should succeed");
        assert!(html.contains("Submit Ratings"));
    }
}
