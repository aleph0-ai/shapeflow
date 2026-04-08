ALTER TABLE human_eval_sessions
    ADD COLUMN used_tools TEXT NOT NULL DEFAULT '[]';

ALTER TABLE human_eval_sessions
    ADD COLUMN used_data_mcp TEXT NOT NULL DEFAULT '[]';

ALTER TABLE human_eval_sessions
    ADD COLUMN used_data_route TEXT NOT NULL DEFAULT '[]';

CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_active_progress
    ON human_eval_sessions (completed, current_item_index);

CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_seed
    ON human_eval_sessions (seed);

CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_active_next_progress
    ON human_eval_sessions (completed, next_question_index);
