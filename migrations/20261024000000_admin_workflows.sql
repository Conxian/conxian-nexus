-- Administrative workflow projections. Never protocol settlement authority.
CREATE TABLE admin_workflow_entities (
    kind TEXT NOT NULL CHECK (kind IN ('release', 'governance')),
    id TEXT NOT NULL CHECK (length(id) BETWEEN 1 AND 256),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 256),
    owner TEXT NOT NULL CHECK (length(owner) BETWEEN 1 AND 256),
    status TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (kind, id),
    CHECK ((kind = 'release' AND status IN ('draft', 'in_review', 'approved', 'rejected', 'published'))
        OR (kind = 'governance' AND status IN ('draft', 'pending', 'approved', 'rejected', 'changes_requested')))
);

CREATE TABLE admin_workflow_events (
    id TEXT PRIMARY KEY,
    command_id TEXT NOT NULL UNIQUE CHECK (length(command_id) BETWEEN 1 AND 256),
    fingerprint TEXT NOT NULL CHECK (length(fingerprint) = 64),
    category TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    actor TEXT NOT NULL,
    summary TEXT NOT NULL,
    notes TEXT CHECK (notes IS NULL OR length(notes) <= 4096),
    timestamp TIMESTAMPTZ NOT NULL,
    receipt_json TEXT NOT NULL,
    FOREIGN KEY (category, entity_id) REFERENCES admin_workflow_entities(kind, id)
);
CREATE INDEX admin_workflow_events_order ON admin_workflow_events(timestamp DESC, id);
