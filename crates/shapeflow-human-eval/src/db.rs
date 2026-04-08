use anyhow::{Context, Result};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{PgPool, Row, SqlitePool, migrate::Migrator};
use std::str::FromStr;

use crate::{
    HumanEvalDatabaseConfig,
    flow::{Difficulty, ModalityOrder, ModalityTargets},
};

static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");
static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite");

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub seed: i64,
    pub difficulty: String,
    pub is_human: bool,
    pub show_answer_validation: bool,
    pub current_item_index: i32,
    pub next_question_index: i32,
    pub image_target: String,
    pub video_target: String,
    pub text_target: String,
    pub tabular_target: String,
    pub sound_target: String,
    pub modality_order: String,
    pub completed: bool,
}

#[derive(Clone)]
pub enum DbPool {
    Postgres(PgPool),
    Sqlite(SqlitePool),
}

pub async fn connect_pool(database: &HumanEvalDatabaseConfig) -> Result<DbPool> {
    match database {
        HumanEvalDatabaseConfig::PostgresUrl(url) => {
            let pool = PgPool::connect(url)
                .await
                .context("failed to connect to postgres")?;
            Ok(DbPool::Postgres(pool))
        }
        HumanEvalDatabaseConfig::SqlitePath(path) => {
            let sqlite_url = sqlite_url_from_path(path);
            let options = SqliteConnectOptions::from_str(&sqlite_url)
                .with_context(|| format!("failed to parse sqlite connection string: {sqlite_url}"))?
                .create_if_missing(true);
            let pool = SqlitePool::connect_with(options)
                .await
                .with_context(|| format!("failed to connect to sqlite at {sqlite_url}"))?;
            Ok(DbPool::Sqlite(pool))
        }
    }
}

pub async fn ensure_schema(pool: &DbPool) -> Result<()> {
    match pool {
        DbPool::Postgres(pg) => POSTGRES_MIGRATOR
            .run(pg)
            .await
            .context("failed to run postgres schema migrations")?,
        DbPool::Sqlite(sqlite) => SQLITE_MIGRATOR
            .run(sqlite)
            .await
            .context("failed to run sqlite schema migrations")?,
    }

    Ok(())
}

pub async fn create_session(
    pool: &DbPool,
    session_id: &str,
    seed: i64,
    difficulty: Difficulty,
    is_human: bool,
    show_answer_validation: bool,
    identifier: Option<&str>,
    initial_item_index: i32,
    modality_targets: &ModalityTargets,
    modality_order: &ModalityOrder,
) -> Result<SessionRecord> {
    let modality_order_serialized = crate::flow::serialize_modality_order(modality_order);
    let sql = if is_postgres(pool) {
        r#"INSERT INTO human_eval_sessions (
            session_id,
            seed,
            difficulty,
            is_human,
            show_answer_validation,
            identifier,
            current_item_index,
            next_question_index,
            image_target,
            video_target,
            text_target,
            tabular_target,
            sound_target,
            modality_order
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING
            session_id,
            seed,
            difficulty,
            is_human,
            show_answer_validation,
            current_item_index,
            next_question_index,
            image_target,
            video_target,
            text_target,
            tabular_target,
            sound_target,
            modality_order,
            completed"#
    } else {
        r#"INSERT INTO human_eval_sessions (
            session_id,
            seed,
            difficulty,
            is_human,
            show_answer_validation,
            identifier,
            current_item_index,
            next_question_index,
            image_target,
            video_target,
            text_target,
            tabular_target,
            sound_target,
            modality_order
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        RETURNING
            session_id,
            seed,
            difficulty,
            is_human,
            show_answer_validation,
            current_item_index,
            next_question_index,
            image_target,
            video_target,
            text_target,
            tabular_target,
            sound_target,
            modality_order,
            completed"#
    };

    match pool {
        DbPool::Postgres(pg) => {
            let row = sqlx::query(sql)
                .bind(session_id)
                .bind(seed)
                .bind(difficulty.as_str())
                .bind(is_human)
                .bind(show_answer_validation)
                .bind(identifier)
                .bind(initial_item_index)
                .bind(initial_item_index)
                .bind(modality_targets[0].as_str())
                .bind(modality_targets[1].as_str())
                .bind(modality_targets[2].as_str())
                .bind(modality_targets[3].as_str())
                .bind(modality_targets[4].as_str())
                .bind(&modality_order_serialized)
                .fetch_one(pg)
                .await
                .context("failed to insert new session row")?;
            decode_session_row_pg(&row)
        }
        DbPool::Sqlite(sqlite) => {
            let row = sqlx::query(sql)
                .bind(session_id)
                .bind(seed)
                .bind(difficulty.as_str())
                .bind(is_human)
                .bind(show_answer_validation)
                .bind(identifier)
                .bind(initial_item_index)
                .bind(initial_item_index)
                .bind(modality_targets[0].as_str())
                .bind(modality_targets[1].as_str())
                .bind(modality_targets[2].as_str())
                .bind(modality_targets[3].as_str())
                .bind(modality_targets[4].as_str())
                .bind(&modality_order_serialized)
                .fetch_one(sqlite)
                .await
                .context("failed to insert new session row")?;
            decode_session_row_sqlite(&row)
        }
    }
}

pub async fn get_session(pool: &DbPool, session_id: &str) -> Result<Option<SessionRecord>> {
    let sql = if is_postgres(pool) {
        r#"SELECT
            session_id,
            seed,
            difficulty,
            is_human,
            show_answer_validation,
            current_item_index,
            next_question_index,
            image_target,
            video_target,
            text_target,
            tabular_target,
            sound_target,
            modality_order,
            completed
        FROM human_eval_sessions
        WHERE session_id = $1"#
    } else {
        r#"SELECT
            session_id,
            seed,
            difficulty,
            is_human,
            show_answer_validation,
            current_item_index,
            next_question_index,
            image_target,
            video_target,
            text_target,
            tabular_target,
            sound_target,
            modality_order,
            completed
        FROM human_eval_sessions
        WHERE session_id = ?1"#
    };

    match pool {
        DbPool::Postgres(pg) => {
            let row = sqlx::query(sql)
                .bind(session_id)
                .fetch_optional(pg)
                .await
                .context("failed to read session")?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(decode_session_row_pg(&row)?))
        }
        DbPool::Sqlite(sqlite) => {
            let row = sqlx::query(sql)
                .bind(session_id)
                .fetch_optional(sqlite)
                .await
                .context("failed to read session")?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(decode_session_row_sqlite(&row)?))
        }
    }
}

pub async fn record_answer(
    pool: &DbPool,
    session_id: &str,
    expected_item_index: i32,
    next_item_index: i32,
    modality: &str,
    was_correct: bool,
    count_toward_totals: bool,
) -> Result<SessionRecord> {
    let (correct_field, wrong_field) = match modality {
        "image" => ("image_correct", "image_wrong"),
        "video" => ("video_correct", "video_wrong"),
        "text" => ("text_correct", "text_wrong"),
        "tabular" => ("tabular_correct", "tabular_wrong"),
        "sound" => ("sound_correct", "sound_wrong"),
        _ => anyhow::bail!("invalid modality '{modality}'"),
    };

    let now_expr = now_expr(pool);
    let p1 = placeholder(pool, 1);
    let p2 = placeholder(pool, 2);
    let p3 = placeholder(pool, 3);
    let p4 = placeholder(pool, 4);
    let p5 = placeholder(pool, 5);
    let correct_increment = if count_toward_totals && was_correct {
        1
    } else {
        0
    };
    let wrong_increment = if count_toward_totals && !was_correct {
        1
    } else {
        0
    };

    let query = format!(
        "UPDATE human_eval_sessions
            SET
                current_item_index = {p1},
                next_question_index = {p1},
                {correct_field} = {correct_field} + {p2},
                {wrong_field} = {wrong_field} + {p3},
                updated_at = {now_expr}
            WHERE session_id = {p4}
              AND completed = FALSE
              AND next_question_index = {p5}
            RETURNING
                session_id,
                seed,
                difficulty,
                is_human,
                show_answer_validation,
                current_item_index,
                next_question_index,
                image_target,
                video_target,
                text_target,
                tabular_target,
                sound_target,
                modality_order,
                completed"
    );

    match pool {
        DbPool::Postgres(pg) => {
            let row = sqlx::query(&query)
                .bind(next_item_index)
                .bind(correct_increment)
                .bind(wrong_increment)
                .bind(session_id)
                .bind(expected_item_index)
                .fetch_one(pg)
                .await
                .context("failed to update answer counters for session")?;
            decode_session_row_pg(&row)
        }
        DbPool::Sqlite(sqlite) => {
            let row = sqlx::query(&query)
                .bind(next_item_index)
                .bind(correct_increment)
                .bind(wrong_increment)
                .bind(session_id)
                .bind(expected_item_index)
                .fetch_one(sqlite)
                .await
                .context("failed to update answer counters for session")?;
            decode_session_row_sqlite(&row)
        }
    }
}

pub async fn store_ratings(
    pool: &DbPool,
    session_id: &str,
    image_rating: i16,
    video_rating: i16,
    text_rating: i16,
    tabular_rating: i16,
    sound_rating: i16,
) -> Result<()> {
    let now_expr = now_expr(pool);
    let p1 = placeholder(pool, 1);
    let p2 = placeholder(pool, 2);
    let p3 = placeholder(pool, 3);
    let p4 = placeholder(pool, 4);
    let p5 = placeholder(pool, 5);
    let p6 = placeholder(pool, 6);

    let query = format!(
        "UPDATE human_eval_sessions
            SET image_difficulty_rating = {p1},
                video_difficulty_rating = {p2},
                text_difficulty_rating = {p3},
                tabular_difficulty_rating = {p4},
                sound_difficulty_rating = {p5},
                completed = TRUE,
                updated_at = {now_expr}
            WHERE session_id = {p6}
              AND completed = FALSE"
    );

    match pool {
        DbPool::Postgres(pg) => {
            let mut tx = pg
                .begin()
                .await
                .context("failed to begin ratings transaction")?;

            let update = sqlx::query(&query)
                .bind(image_rating)
                .bind(video_rating)
                .bind(text_rating)
                .bind(tabular_rating)
                .bind(sound_rating)
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .context("failed to write session ratings")?;

            if update.rows_affected() != 1 {
                tx.rollback()
                    .await
                    .context("failed to rollback ratings update")?;
                anyhow::bail!("session {session_id} was already completed or does not exist");
            }

            tx.commit()
                .await
                .context("failed to commit ratings update")?;
        }
        DbPool::Sqlite(sqlite) => {
            let mut tx = sqlite
                .begin()
                .await
                .context("failed to begin ratings transaction")?;

            let update = sqlx::query(&query)
                .bind(image_rating)
                .bind(video_rating)
                .bind(text_rating)
                .bind(tabular_rating)
                .bind(sound_rating)
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .context("failed to write session ratings")?;

            if update.rows_affected() != 1 {
                tx.rollback()
                    .await
                    .context("failed to rollback ratings update")?;
                anyhow::bail!("session {session_id} was already completed or does not exist");
            }

            tx.commit()
                .await
                .context("failed to commit ratings update")?;
        }
    }

    Ok(())
}

pub async fn append_used_tools(
    pool: &DbPool,
    session_id: &str,
    question_number: i32,
) -> Result<()> {
    append_usage_marker(pool, session_id, "used_tools", question_number).await
}

pub async fn append_used_data_mcp(
    pool: &DbPool,
    session_id: &str,
    question_number: i32,
) -> Result<()> {
    append_usage_marker(pool, session_id, "used_data_mcp", question_number).await
}

pub async fn append_used_data_route(
    pool: &DbPool,
    session_id: &str,
    question_number: i32,
) -> Result<()> {
    append_usage_marker(pool, session_id, "used_data_route", question_number).await
}

pub fn parse_difficulty(value: &str) -> Result<Difficulty> {
    Difficulty::from_str(value).context("invalid difficulty")
}

fn decode_session_row_pg(row: &sqlx::postgres::PgRow) -> Result<SessionRecord> {
    Ok(SessionRecord {
        session_id: row.try_get("session_id")?,
        seed: row.try_get("seed")?,
        difficulty: row.try_get("difficulty")?,
        is_human: row.try_get("is_human")?,
        show_answer_validation: row.try_get("show_answer_validation")?,
        current_item_index: row.try_get("current_item_index")?,
        next_question_index: row.try_get("next_question_index")?,
        image_target: row.try_get("image_target")?,
        video_target: row.try_get("video_target")?,
        text_target: row.try_get("text_target")?,
        tabular_target: row.try_get("tabular_target")?,
        sound_target: row.try_get("sound_target")?,
        modality_order: row.try_get("modality_order")?,
        completed: row.try_get("completed")?,
    })
}

fn decode_session_row_sqlite(row: &sqlx::sqlite::SqliteRow) -> Result<SessionRecord> {
    Ok(SessionRecord {
        session_id: row.try_get("session_id")?,
        seed: row.try_get("seed")?,
        difficulty: row.try_get("difficulty")?,
        is_human: row.try_get("is_human")?,
        show_answer_validation: row.try_get("show_answer_validation")?,
        current_item_index: row.try_get("current_item_index")?,
        next_question_index: row.try_get("next_question_index")?,
        image_target: row.try_get("image_target")?,
        video_target: row.try_get("video_target")?,
        text_target: row.try_get("text_target")?,
        tabular_target: row.try_get("tabular_target")?,
        sound_target: row.try_get("sound_target")?,
        modality_order: row.try_get("modality_order")?,
        completed: row.try_get("completed")?,
    })
}

fn sqlite_url_from_path(path: &str) -> String {
    if path == ":memory:" {
        "sqlite::memory:".to_string()
    } else if path.starts_with("sqlite:") {
        path.to_string()
    } else {
        format!("sqlite://{path}")
    }
}

fn is_postgres(pool: &DbPool) -> bool {
    matches!(pool, DbPool::Postgres(_))
}

fn now_expr(pool: &DbPool) -> &'static str {
    if is_postgres(pool) {
        "NOW()"
    } else {
        "CURRENT_TIMESTAMP"
    }
}

fn placeholder(pool: &DbPool, index: usize) -> String {
    if is_postgres(pool) {
        format!("${index}")
    } else {
        format!("?{index}")
    }
}

async fn append_usage_marker(
    pool: &DbPool,
    session_id: &str,
    column_name: &str,
    question_number: i32,
) -> Result<()> {
    match pool {
        DbPool::Postgres(pg) => {
            let mut tx = pg
                .begin()
                .await
                .context("failed to begin usage marker transaction")?;

            let select_sql = format!(
                "SELECT {column_name} FROM human_eval_sessions WHERE session_id = $1 FOR UPDATE"
            );
            let row = sqlx::query(&select_sql)
                .bind(session_id)
                .fetch_optional(&mut *tx)
                .await
                .context("failed to load usage marker array")?;
            let Some(row) = row else {
                tx.rollback()
                    .await
                    .context("failed to rollback missing usage marker session")?;
                anyhow::bail!("session {session_id} not found");
            };

            let mut values: Vec<i32> = row
                .try_get(column_name)
                .with_context(|| format!("failed to decode {column_name} as integer array"))?;
            if !values.contains(&question_number) {
                values.push(question_number);
            }

            let update_sql = format!(
                "UPDATE human_eval_sessions
                 SET {column_name} = $1,
                     updated_at = NOW()
                 WHERE session_id = $2"
            );
            sqlx::query(&update_sql)
                .bind(values)
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .context("failed to update usage marker array")?;

            tx.commit()
                .await
                .context("failed to commit usage marker transaction")?;
        }
        DbPool::Sqlite(sqlite) => {
            let mut tx = sqlite
                .begin()
                .await
                .context("failed to begin usage marker transaction")?;

            let select_sql =
                format!("SELECT {column_name} FROM human_eval_sessions WHERE session_id = ?1");
            let row = sqlx::query(&select_sql)
                .bind(session_id)
                .fetch_optional(&mut *tx)
                .await
                .context("failed to load usage marker json")?;
            let Some(row) = row else {
                tx.rollback()
                    .await
                    .context("failed to rollback missing usage marker session")?;
                anyhow::bail!("session {session_id} not found");
            };

            let raw_json: String = row
                .try_get(column_name)
                .with_context(|| format!("failed to decode {column_name} as text"))?;
            let mut values: Vec<i32> = serde_json::from_str(&raw_json)
                .with_context(|| format!("failed to parse {column_name} json array"))?;
            if !values.contains(&question_number) {
                values.push(question_number);
            }
            let next_json =
                serde_json::to_string(&values).context("failed to serialize usage marker array")?;

            let update_sql = format!(
                "UPDATE human_eval_sessions
                 SET {column_name} = ?1,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE session_id = ?2"
            );
            sqlx::query(&update_sql)
                .bind(next_json)
                .bind(session_id)
                .execute(&mut *tx)
                .await
                .context("failed to update usage marker json")?;

            tx.commit()
                .await
                .context("failed to commit usage marker transaction")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{Difficulty, QuestionTarget, canonical_modality_order};
    use sqlx::Row;

    #[tokio::test]
    async fn record_answer_counts_scored_items() {
        let pool = connect_pool(&HumanEvalDatabaseConfig::SqlitePath(":memory:".to_string()))
            .await
            .expect("sqlite pool");
        ensure_schema(&pool).await.expect("schema");

        let targets = [QuestionTarget::OrderedQuadrantPassage; 5];
        let order = canonical_modality_order();
        create_session(
            &pool,
            "session-counted",
            123,
            Difficulty::Easy,
            true,
            false,
            None,
            0,
            &targets,
            &order,
        )
        .await
        .expect("create session");

        record_answer(&pool, "session-counted", 0, 1, "image", true, true)
            .await
            .expect("record scored answer");

        let DbPool::Sqlite(sqlite) = &pool else {
            panic!("expected sqlite pool")
        };
        let row = sqlx::query(
            "SELECT image_correct, image_wrong, next_question_index
             FROM human_eval_sessions
             WHERE session_id = ?1",
        )
        .bind("session-counted")
        .fetch_one(sqlite)
        .await
        .expect("select counters");

        let image_correct: i64 = row.try_get("image_correct").expect("image_correct");
        let image_wrong: i64 = row.try_get("image_wrong").expect("image_wrong");
        let next_question_index: i64 = row
            .try_get("next_question_index")
            .expect("next_question_index");
        assert_eq!(image_correct, 1);
        assert_eq!(image_wrong, 0);
        assert_eq!(next_question_index, 1);
    }

    #[tokio::test]
    async fn record_answer_skips_practice_item_counters() {
        let pool = connect_pool(&HumanEvalDatabaseConfig::SqlitePath(":memory:".to_string()))
            .await
            .expect("sqlite pool");
        ensure_schema(&pool).await.expect("schema");

        let targets = [QuestionTarget::OrderedQuadrantPassage; 5];
        let order = canonical_modality_order();
        create_session(
            &pool,
            "session-practice",
            123,
            Difficulty::Easy,
            true,
            false,
            None,
            0,
            &targets,
            &order,
        )
        .await
        .expect("create session");

        record_answer(&pool, "session-practice", 0, 1, "image", false, false)
            .await
            .expect("record practice answer");

        let DbPool::Sqlite(sqlite) = &pool else {
            panic!("expected sqlite pool")
        };
        let row = sqlx::query(
            "SELECT image_correct, image_wrong, next_question_index
             FROM human_eval_sessions
             WHERE session_id = ?1",
        )
        .bind("session-practice")
        .fetch_one(sqlite)
        .await
        .expect("select counters");

        let image_correct: i64 = row.try_get("image_correct").expect("image_correct");
        let image_wrong: i64 = row.try_get("image_wrong").expect("image_wrong");
        let next_question_index: i64 = row
            .try_get("next_question_index")
            .expect("next_question_index");
        assert_eq!(image_correct, 0);
        assert_eq!(image_wrong, 0);
        assert_eq!(next_question_index, 1);
    }

    #[tokio::test]
    async fn ensure_schema_creates_mcp_http_sessions_table() {
        let pool = connect_pool(&HumanEvalDatabaseConfig::SqlitePath(":memory:".to_string()))
            .await
            .expect("sqlite pool");
        ensure_schema(&pool).await.expect("schema");

        let DbPool::Sqlite(sqlite) = &pool else {
            panic!("expected sqlite pool");
        };

        let table = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'mcp_http_sessions'",
        )
        .fetch_one(sqlite)
        .await
        .expect("mcp_http_sessions table");
        let table_name: String = table.try_get("name").expect("table name");
        assert_eq!(table_name, "mcp_http_sessions");

        let columns = sqlx::query("PRAGMA table_info(mcp_http_sessions)")
            .fetch_all(sqlite)
            .await
            .expect("table columns");
        let column_names: Vec<String> = columns
            .into_iter()
            .map(|row| row.try_get("name").expect("column name"))
            .collect();
        assert!(
            column_names
                .iter()
                .any(|name| name == "initialize_request_json"),
            "initialize_request_json column exists"
        );
        assert!(
            column_names.iter().any(|name| name == "rehydrate_count"),
            "rehydrate_count column exists"
        );
    }

    #[tokio::test]
    async fn append_usage_markers_updates_json_arrays_in_sqlite() {
        let pool = connect_pool(&HumanEvalDatabaseConfig::SqlitePath(":memory:".to_string()))
            .await
            .expect("sqlite pool");
        ensure_schema(&pool).await.expect("schema");

        let targets = [QuestionTarget::OrderedQuadrantPassage; 5];
        let order = canonical_modality_order();
        create_session(
            &pool,
            "session-usage",
            321,
            Difficulty::Medium,
            false,
            false,
            None,
            0,
            &targets,
            &order,
        )
        .await
        .expect("create session");

        append_used_tools(&pool, "session-usage", 0)
            .await
            .expect("append used_tools");
        append_used_data_mcp(&pool, "session-usage", 0)
            .await
            .expect("append used_data_mcp");
        append_used_data_route(&pool, "session-usage", 0)
            .await
            .expect("append used_data_route");
        append_used_data_mcp(&pool, "session-usage", 0)
            .await
            .expect("append duplicate used_data_mcp");
        append_used_data_route(&pool, "session-usage", 0)
            .await
            .expect("append duplicate used_data_route");

        let DbPool::Sqlite(sqlite) = &pool else {
            panic!("expected sqlite pool");
        };
        let row = sqlx::query(
            "SELECT used_tools, used_data_mcp, used_data_route
             FROM human_eval_sessions
             WHERE session_id = ?1",
        )
        .bind("session-usage")
        .fetch_one(sqlite)
        .await
        .expect("select usage columns");

        let used_tools: String = row.try_get("used_tools").expect("used_tools");
        let used_data_mcp: String = row.try_get("used_data_mcp").expect("used_data_mcp");
        let used_data_route: String = row.try_get("used_data_route").expect("used_data_route");

        assert_eq!(
            serde_json::from_str::<Vec<i32>>(&used_tools).expect("decode used_tools"),
            vec![0]
        );
        assert_eq!(
            serde_json::from_str::<Vec<i32>>(&used_data_mcp).expect("decode used_data_mcp"),
            vec![0]
        );
        assert_eq!(
            serde_json::from_str::<Vec<i32>>(&used_data_route).expect("decode used_data_route"),
            vec![0]
        );
    }

    #[tokio::test]
    async fn ensure_schema_creates_session_indexes_and_usage_columns() {
        let pool = connect_pool(&HumanEvalDatabaseConfig::SqlitePath(":memory:".to_string()))
            .await
            .expect("sqlite pool");
        ensure_schema(&pool).await.expect("schema");

        let DbPool::Sqlite(sqlite) = &pool else {
            panic!("expected sqlite pool");
        };
        let table_columns = sqlx::query("PRAGMA table_info(human_eval_sessions)")
            .fetch_all(sqlite)
            .await
            .expect("human_eval_sessions columns");
        let column_names: Vec<String> = table_columns
            .into_iter()
            .map(|row| row.try_get("name").expect("column name"))
            .collect();
        assert!(column_names.iter().any(|name| name == "used_tools"));
        assert!(column_names.iter().any(|name| name == "used_data_mcp"));
        assert!(column_names.iter().any(|name| name == "used_data_route"));

        let indexes = sqlx::query("PRAGMA index_list('human_eval_sessions')")
            .fetch_all(sqlite)
            .await
            .expect("human_eval_sessions indexes");
        let index_names: Vec<String> = indexes
            .into_iter()
            .map(|row| row.try_get("name").expect("index name"))
            .collect();
        assert!(
            index_names
                .iter()
                .any(|name| name == "idx_human_eval_sessions_active_progress")
        );
        assert!(
            index_names
                .iter()
                .any(|name| name == "idx_human_eval_sessions_seed")
        );
        assert!(
            index_names
                .iter()
                .any(|name| name == "idx_human_eval_sessions_active_next_progress")
        );
    }

    #[tokio::test]
    async fn sqlite_cleanup_trigger_removes_stale_mcp_sessions_on_write() {
        let pool = connect_pool(&HumanEvalDatabaseConfig::SqlitePath(":memory:".to_string()))
            .await
            .expect("sqlite pool");
        ensure_schema(&pool).await.expect("schema");

        let DbPool::Sqlite(sqlite) = &pool else {
            panic!("expected sqlite pool");
        };

        sqlx::query(
            "INSERT INTO mcp_http_sessions (
                session_id,
                initialized,
                closed,
                initialize_request_json,
                rehydrate_count,
                created_at,
                updated_at,
                last_seen_at
            ) VALUES (?1, 1, 0, '{}', 0, ?2, ?2, ?2)",
        )
        .bind("stale-session")
        .bind("2000-01-01 00:00:00")
        .execute(sqlite)
        .await
        .expect("insert stale row");

        sqlx::query(
            "INSERT INTO mcp_http_sessions (session_id, initialized, closed, initialize_request_json, rehydrate_count)
             VALUES (?1, 0, 0, NULL, 0)",
        )
        .bind("fresh-session")
        .execute(sqlite)
        .await
        .expect("insert fresh row");

        let stale = sqlx::query("SELECT session_id FROM mcp_http_sessions WHERE session_id = ?1")
            .bind("stale-session")
            .fetch_optional(sqlite)
            .await
            .expect("query stale row");
        let fresh = sqlx::query("SELECT session_id FROM mcp_http_sessions WHERE session_id = ?1")
            .bind("fresh-session")
            .fetch_optional(sqlite)
            .await
            .expect("query fresh row");

        assert!(stale.is_none(), "stale session row should be cleaned up");
        assert!(fresh.is_some(), "fresh session row should remain");
    }
}
