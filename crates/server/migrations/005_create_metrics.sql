CREATE TABLE IF NOT EXISTS metrics (
    id UUID NOT NULL DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL,
    host_id UUID NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    metric_name TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    unit TEXT NOT NULL,
    attributes JSONB DEFAULT '{}',
    PRIMARY KEY (id, timestamp)
) PARTITION BY RANGE (timestamp);

CREATE INDEX IF NOT EXISTS idx_metrics_project_name_ts ON metrics (project_id, metric_name, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_metrics_host_ts ON metrics (host_id, timestamp DESC);
