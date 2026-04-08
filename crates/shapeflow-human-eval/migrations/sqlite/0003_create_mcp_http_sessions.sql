CREATE TABLE IF NOT EXISTS mcp_http_sessions (
    session_id TEXT PRIMARY KEY,
    initialized INTEGER NOT NULL DEFAULT 0,
    closed INTEGER NOT NULL DEFAULT 0,
    initialize_request_json TEXT NULL,
    rehydrate_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TEXT NULL
);

CREATE INDEX IF NOT EXISTS idx_mcp_http_sessions_closed_initialized
    ON mcp_http_sessions (closed, initialized);

DROP TRIGGER IF EXISTS trg_cleanup_mcp_http_sessions_insert;
CREATE TRIGGER trg_cleanup_mcp_http_sessions_insert
    BEFORE INSERT ON mcp_http_sessions
BEGIN
    DELETE FROM mcp_http_sessions
     WHERE COALESCE(last_seen_at, created_at) < datetime('now', '-15 minutes');
END;

DROP TRIGGER IF EXISTS trg_cleanup_mcp_http_sessions_update;
CREATE TRIGGER trg_cleanup_mcp_http_sessions_update
    BEFORE UPDATE ON mcp_http_sessions
BEGIN
    DELETE FROM mcp_http_sessions
     WHERE COALESCE(last_seen_at, created_at) < datetime('now', '-15 minutes');
END;
