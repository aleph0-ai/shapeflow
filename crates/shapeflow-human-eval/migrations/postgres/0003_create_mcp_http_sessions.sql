CREATE TABLE IF NOT EXISTS mcp_http_sessions (
    session_id TEXT PRIMARY KEY,
    initialized BOOLEAN NOT NULL DEFAULT FALSE,
    closed BOOLEAN NOT NULL DEFAULT FALSE,
    initialize_request_json TEXT NULL,
    rehydrate_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_mcp_http_sessions_closed_initialized
    ON mcp_http_sessions (closed, initialized);

CREATE OR REPLACE FUNCTION cleanup_stale_mcp_http_sessions()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    DELETE FROM mcp_http_sessions
     WHERE COALESCE(last_seen_at, created_at) < NOW() - INTERVAL '15 minutes';
    RETURN NULL;
END;
$$;

DROP TRIGGER IF EXISTS trg_cleanup_mcp_http_sessions ON mcp_http_sessions;
CREATE TRIGGER trg_cleanup_mcp_http_sessions
BEFORE INSERT OR UPDATE ON mcp_http_sessions
FOR EACH STATEMENT
EXECUTE FUNCTION cleanup_stale_mcp_http_sessions();
