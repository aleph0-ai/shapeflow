ALTER TABLE human_eval_sessions
    ADD COLUMN IF NOT EXISTS skipped_questions INTEGER[] NOT NULL DEFAULT ARRAY[]::INTEGER[];

UPDATE human_eval_sessions
   SET next_question_index = current_item_index
 WHERE next_question_index IS NULL
    OR next_question_index <> current_item_index;

DROP INDEX IF EXISTS idx_human_eval_sessions_progress;
DROP INDEX IF EXISTS idx_human_eval_sessions_active_progress;

ALTER TABLE human_eval_sessions
    DROP COLUMN current_item_index;
