CREATE TABLE IF NOT EXISTS spans (
    id TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    parent_span_id TEXT,
    project_id UUID NOT NULL,
    host_id UUID NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    duration_ms DOUBLE PRECISION NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'internal',
    status TEXT NOT NULL DEFAULT 'ok',
    status_message TEXT,
    attributes JSONB DEFAULT '{}',
    PRIMARY KEY (id, timestamp)
) PARTITION BY RANGE (timestamp);

CREATE INDEX IF NOT EXISTS idx_spans_trace_id ON spans (trace_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_spans_project_ts ON spans (project_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_spans_status ON spans (project_id, status, timestamp DESC);
