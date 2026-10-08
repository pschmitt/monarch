-- Per-check (service) overrides of the generic notification settings.
CREATE TABLE check_alerts (
    host_id INTEGER NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    service TEXT NOT NULL,
    muted INTEGER NOT NULL DEFAULT 0,
    -- JSON array of event kinds that notify for this check; NULL = follow the global settings
    events TEXT,
    -- JSON array of channel ids that receive this check's events; NULL = normal routing
    channels TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (host_id, service)
);
