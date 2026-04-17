CREATE TABLE IF NOT EXISTS runs (
    run_id UUID PRIMARY KEY,
    schema_version TEXT NOT NULL,
    brief_id UUID NOT NULL,
    status TEXT NOT NULL,
    trigger TEXT NOT NULL,
    title TEXT NOT NULL,
    requested_by TEXT,
    selected_pack TEXT,
    repository_host TEXT,
    repository_owner TEXT,
    repository_name TEXT,
    repository_default_branch TEXT,
    repository_visibility TEXT,
    goal_count INTEGER NOT NULL,
    functional_requirement_count INTEGER NOT NULL,
    constraint_count INTEGER NOT NULL,
    brief_source_path TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_runs_brief_id ON runs (brief_id);
CREATE INDEX IF NOT EXISTS idx_runs_created_at ON runs (created_at DESC);

CREATE TABLE IF NOT EXISTS artifacts (
    artifact_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES runs (run_id) ON DELETE CASCADE,
    type TEXT NOT NULL,
    format TEXT NOT NULL,
    location_kind TEXT NOT NULL,
    location_value TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    labels JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_artifacts_run_id ON artifacts (run_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_type ON artifacts (type);

CREATE TABLE IF NOT EXISTS tasks (
    task_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES runs (run_id) ON DELETE CASCADE,
    backlog_item_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    priority TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    status TEXT NOT NULL,
    execution JSONB NOT NULL DEFAULT '{}'::jsonb,
    dependency_task_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    source_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
    assigned_pack TEXT,
    approval_required BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    failure_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tasks_run_id ON tasks (run_id);
CREATE INDEX IF NOT EXISTS idx_tasks_kind ON tasks (kind);

ALTER TABLE tasks ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS failure_reason TEXT;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS execution JSONB NOT NULL DEFAULT '{}'::jsonb;
