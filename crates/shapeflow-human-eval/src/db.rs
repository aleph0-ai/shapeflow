use anyhow::{Context, Result};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{PgPool, Row, SqlitePool};
use std::str::FromStr;

use crate::{
    HumanEvalDatabaseConfig,
    flow::{Difficulty, ModalityOrder, ModalityTargets},
};

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
    execute(pool, create_table_sql(pool))
        .await
        .context("failed to create or verify human_eval_sessions table")?;

    execute(
        pool,
        r#"CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_active_progress
           ON human_eval_sessions (completed, current_item_index)"#,
    )
    .await
    .context("failed to create idx_human_eval_sessions_active_progress")?;

    execute(
        pool,
        r#"CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_seed
           ON human_eval_sessions (seed)"#,
    )
    .await
    .context("failed to create idx_human_eval_sessions_seed")?;

    add_column_if_missing(pool, "next_question_index", "INTEGER NOT NULL DEFAULT 0")
        .await
        .context("failed to ensure next_question_index column")?;
    add_column_if_missing(pool, "image_target", "TEXT NOT NULL DEFAULT 'oqp'")
        .await
        .context("failed to ensure image_target column")?;
    add_column_if_missing(pool, "video_target", "TEXT NOT NULL DEFAULT 'oqp'")
        .await
        .context("failed to ensure video_target column")?;
    add_column_if_missing(pool, "text_target", "TEXT NOT NULL DEFAULT 'oqp'")
        .await
        .context("failed to ensure text_target column")?;
    add_column_if_missing(pool, "tabular_target", "TEXT NOT NULL DEFAULT 'oqp'")
        .await
        .context("failed to ensure tabular_target column")?;
    add_column_if_missing(pool, "sound_target", "TEXT NOT NULL DEFAULT 'oqp'")
        .await
        .context("failed to ensure sound_target column")?;
    add_column_if_missing(pool, "modality_order", "TEXT NOT NULL DEFAULT '0,1,2,3,4'")
        .await
        .context("failed to ensure modality_order column")?;

    if is_postgres(pool) {
        execute(
            pool,
            r#"ALTER TABLE human_eval_sessions
               DROP COLUMN IF EXISTS start_time"#,
        )
        .await
        .context("failed to drop start_time column")?;
    }

    if has_column(pool, "next_question_index")
        .await
        .context("failed to check next_question_index column")?
    {
        execute(
            pool,
            r#"CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_active_next_progress
               ON human_eval_sessions (completed, next_question_index)"#,
        )
        .await
        .context("failed to create idx_human_eval_sessions_active_next_progress")?;
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

pub fn parse_difficulty(value: &str) -> Result<Difficulty> {
    Difficulty::from_str(value).context("invalid difficulty")
}

fn create_table_sql(pool: &DbPool) -> &'static str {
    if is_postgres(pool) {
        r#"CREATE TABLE IF NOT EXISTS human_eval_sessions (
            session_id TEXT PRIMARY KEY,
            seed BIGINT NOT NULL,
            difficulty TEXT NOT NULL CHECK (difficulty IN ('easy', 'medium', 'hard')),
            is_human BOOLEAN NOT NULL,
            show_answer_validation BOOLEAN NOT NULL,
            identifier TEXT NULL,
            image_target TEXT NOT NULL DEFAULT 'oqp',
            video_target TEXT NOT NULL DEFAULT 'oqp',
            text_target TEXT NOT NULL DEFAULT 'oqp',
            tabular_target TEXT NOT NULL DEFAULT 'oqp',
            sound_target TEXT NOT NULL DEFAULT 'oqp',
            modality_order TEXT NOT NULL DEFAULT '0,1,2,3,4',
            current_item_index INTEGER NOT NULL DEFAULT 0,
            next_question_index INTEGER NOT NULL DEFAULT 0,
            completed BOOLEAN NOT NULL DEFAULT FALSE,

            image_correct INTEGER NOT NULL DEFAULT 0,
            image_wrong INTEGER NOT NULL DEFAULT 0,
            video_correct INTEGER NOT NULL DEFAULT 0,
            video_wrong INTEGER NOT NULL DEFAULT 0,
            text_correct INTEGER NOT NULL DEFAULT 0,
            text_wrong INTEGER NOT NULL DEFAULT 0,
            tabular_correct INTEGER NOT NULL DEFAULT 0,
            tabular_wrong INTEGER NOT NULL DEFAULT 0,
            sound_correct INTEGER NOT NULL DEFAULT 0,
            sound_wrong INTEGER NOT NULL DEFAULT 0,

            image_difficulty_rating SMALLINT CHECK (image_difficulty_rating IS NULL OR image_difficulty_rating BETWEEN 1 AND 5),
            video_difficulty_rating SMALLINT CHECK (video_difficulty_rating IS NULL OR video_difficulty_rating BETWEEN 1 AND 5),
            text_difficulty_rating SMALLINT CHECK (text_difficulty_rating IS NULL OR text_difficulty_rating BETWEEN 1 AND 5),
            tabular_difficulty_rating SMALLINT CHECK (tabular_difficulty_rating IS NULL OR tabular_difficulty_rating BETWEEN 1 AND 5),
            sound_difficulty_rating SMALLINT CHECK (sound_difficulty_rating IS NULL OR sound_difficulty_rating BETWEEN 1 AND 5),

            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#
    } else {
        r#"CREATE TABLE IF NOT EXISTS human_eval_sessions (
            session_id TEXT PRIMARY KEY,
            seed INTEGER NOT NULL,
            difficulty TEXT NOT NULL CHECK (difficulty IN ('easy', 'medium', 'hard')),
            is_human INTEGER NOT NULL,
            show_answer_validation INTEGER NOT NULL,
            identifier TEXT NULL,
            image_target TEXT NOT NULL DEFAULT 'oqp',
            video_target TEXT NOT NULL DEFAULT 'oqp',
            text_target TEXT NOT NULL DEFAULT 'oqp',
            tabular_target TEXT NOT NULL DEFAULT 'oqp',
            sound_target TEXT NOT NULL DEFAULT 'oqp',
            modality_order TEXT NOT NULL DEFAULT '0,1,2,3,4',
            current_item_index INTEGER NOT NULL DEFAULT 0,
            next_question_index INTEGER NOT NULL DEFAULT 0,
            completed INTEGER NOT NULL DEFAULT 0,

            image_correct INTEGER NOT NULL DEFAULT 0,
            image_wrong INTEGER NOT NULL DEFAULT 0,
            video_correct INTEGER NOT NULL DEFAULT 0,
            video_wrong INTEGER NOT NULL DEFAULT 0,
            text_correct INTEGER NOT NULL DEFAULT 0,
            text_wrong INTEGER NOT NULL DEFAULT 0,
            tabular_correct INTEGER NOT NULL DEFAULT 0,
            tabular_wrong INTEGER NOT NULL DEFAULT 0,
            sound_correct INTEGER NOT NULL DEFAULT 0,
            sound_wrong INTEGER NOT NULL DEFAULT 0,

            image_difficulty_rating INTEGER CHECK (image_difficulty_rating IS NULL OR image_difficulty_rating BETWEEN 1 AND 5),
            video_difficulty_rating INTEGER CHECK (video_difficulty_rating IS NULL OR video_difficulty_rating BETWEEN 1 AND 5),
            text_difficulty_rating INTEGER CHECK (text_difficulty_rating IS NULL OR text_difficulty_rating BETWEEN 1 AND 5),
            tabular_difficulty_rating INTEGER CHECK (tabular_difficulty_rating IS NULL OR tabular_difficulty_rating BETWEEN 1 AND 5),
            sound_difficulty_rating INTEGER CHECK (sound_difficulty_rating IS NULL OR sound_difficulty_rating BETWEEN 1 AND 5),

            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#
    }
}

async fn execute(pool: &DbPool, sql: &str) -> Result<()> {
    match pool {
        DbPool::Postgres(pg) => {
            sqlx::query(sql).execute(pg).await?;
        }
        DbPool::Sqlite(sqlite) => {
            sqlx::query(sql).execute(sqlite).await?;
        }
    }
    Ok(())
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

async fn has_column(pool: &DbPool, column_name: &str) -> Result<bool> {
    match pool {
        DbPool::Postgres(pg) => {
            let row = sqlx::query(
                r#"SELECT 1
                   FROM information_schema.columns
                   WHERE table_name = $1
                     AND column_name = $2
                   LIMIT 1"#,
            )
            .bind("human_eval_sessions")
            .bind(column_name)
            .fetch_optional(pg)
            .await
            .context("failed to query postgres column metadata")?;
            Ok(row.is_some())
        }
        DbPool::Sqlite(sqlite) => {
            let row = sqlx::query(
                "SELECT 1 FROM pragma_table_info('human_eval_sessions') WHERE name = ?1 LIMIT 1",
            )
            .bind(column_name)
            .fetch_optional(sqlite)
            .await
            .context("failed to query sqlite column metadata")?;
            Ok(row.is_some())
        }
    }
}

async fn add_column_if_missing(
    pool: &DbPool,
    column_name: &str,
    column_definition: &str,
) -> Result<()> {
    if has_column(pool, column_name).await? {
        return Ok(());
    }

    let sql =
        format!("ALTER TABLE human_eval_sessions ADD COLUMN {column_name} {column_definition}");
    execute(pool, &sql).await?;
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
}
