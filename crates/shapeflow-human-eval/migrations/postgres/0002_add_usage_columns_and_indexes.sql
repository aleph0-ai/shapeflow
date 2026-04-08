ALTER TABLE human_eval_sessions
    ADD COLUMN IF NOT EXISTS used_tools INTEGER[] NOT NULL DEFAULT ARRAY[]::INTEGER[];

ALTER TABLE human_eval_sessions
    ADD COLUMN IF NOT EXISTS used_data_mcp INTEGER[] NOT NULL DEFAULT ARRAY[]::INTEGER[];

ALTER TABLE human_eval_sessions
    ADD COLUMN IF NOT EXISTS used_data_route INTEGER[] NOT NULL DEFAULT ARRAY[]::INTEGER[];

CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_active_progress
    ON human_eval_sessions (completed, current_item_index);

CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_seed
    ON human_eval_sessions (seed);

CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_active_next_progress
    ON human_eval_sessions (completed, next_question_index);
