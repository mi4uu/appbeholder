CREATE TABLE IF NOT EXISTS log_entries (
    id UUID NOT NULL DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL,
    host_id UUID NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'backend',
    trace_id TEXT,
    span_id TEXT,
    fingerprint TEXT,
    attributes JSONB DEFAULT '{}',
    stack_trace TEXT,
    PRIMARY KEY (id, timestamp)
) PARTITION BY RANGE (timestamp);

CREATE INDEX IF NOT EXISTS idx_log_entries_project_ts ON log_entries (project_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_log_entries_level ON log_entries (project_id, level, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_log_entries_trace_id ON log_entries (trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_log_entries_fingerprint ON log_entries (project_id, fingerprint, timestamp DESC) WHERE fingerprint IS NOT NULL;
