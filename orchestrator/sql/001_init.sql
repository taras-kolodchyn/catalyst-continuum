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

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    provider TEXT NOT NULL,
    delivery_id TEXT PRIMARY KEY,
    event TEXT NOT NULL,
    action TEXT,
    repository_full_name TEXT,
    repository_default_branch TEXT,
    installation_id BIGINT,
    ref_name TEXT,
    routing_status TEXT NOT NULL,
    routing_action TEXT,
    routing_reason TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    payload_bytes BIGINT NOT NULL,
    signature_verified BOOLEAN NOT NULL DEFAULT FALSE,
    status TEXT NOT NULL,
    outcome TEXT NOT NULL,
    receipt_path TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_created_at ON webhook_deliveries (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_event ON webhook_deliveries (event);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_repository ON webhook_deliveries (repository_full_name);

ALTER TABLE tasks ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS failure_reason TEXT;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS execution JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS provider TEXT NOT NULL DEFAULT 'github';
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS action TEXT;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS repository_full_name TEXT;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS repository_default_branch TEXT;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS installation_id BIGINT;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS ref_name TEXT;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS routing_status TEXT NOT NULL DEFAULT 'not_evaluated';
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS routing_action TEXT;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS routing_reason TEXT NOT NULL DEFAULT '';
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS payload_digest TEXT NOT NULL DEFAULT '';
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS payload_bytes BIGINT NOT NULL DEFAULT 0;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS signature_verified BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'accepted';
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS outcome TEXT NOT NULL DEFAULT 'accepted';
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS receipt_path TEXT NOT NULL DEFAULT '';
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS message TEXT NOT NULL DEFAULT '';
ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
