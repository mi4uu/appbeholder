CREATE TABLE IF NOT EXISTS error_groups (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    message TEXT NOT NULL,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    count BIGINT NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',
    hosts TEXT[] NOT NULL DEFAULT '{}',
    UNIQUE(project_id, fingerprint)
);
CREATE INDEX IF NOT EXISTS idx_error_groups_project_status ON error_groups(project_id, status, last_seen DESC);
