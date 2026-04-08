CREATE TABLE IF NOT EXISTS human_eval_sessions (
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
);

CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_active_progress
    ON human_eval_sessions (completed, current_item_index);

CREATE INDEX IF NOT EXISTS idx_human_eval_sessions_seed
    ON human_eval_sessions (seed);
