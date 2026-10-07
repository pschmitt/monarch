CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'viewer',
    created_at INTEGER NOT NULL,
    last_login INTEGER
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE hosts (
    id INTEGER PRIMARY KEY,
    monit_id TEXT NOT NULL UNIQUE,
    hostname TEXT NOT NULL,
    display_name TEXT,
    description TEXT,
    monit_version TEXT,
    incarnation INTEGER,
    monit_uptime INTEGER,
    poll INTEGER NOT NULL DEFAULT 30,
    startdelay INTEGER,
    controlfile TEXT,
    os_name TEXT,
    os_release TEXT,
    os_version TEXT,
    machine TEXT,
    cpu_count INTEGER,
    mem_total_kb INTEGER,
    swap_total_kb INTEGER,
    httpd_address TEXT,
    httpd_port INTEGER,
    httpd_ssl INTEGER NOT NULL DEFAULT 0,
    httpd_unixsocket TEXT,
    reported_username TEXT,
    reported_password TEXT,
    override_url TEXT,
    override_username TEXT,
    override_password TEXT,
    tls_skip_verify INTEGER NOT NULL DEFAULT 1,
    hostgroups TEXT NOT NULL DEFAULT '[]',
    servicegroups TEXT NOT NULL DEFAULT '{}',
    remote_addr TEXT,
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    online INTEGER NOT NULL DEFAULT 1,
    muted_until INTEGER
);

CREATE TABLE services (
    id INTEGER PRIMARY KEY,
    host_id INTEGER NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    type INTEGER NOT NULL,
    status INTEGER NOT NULL DEFAULT 0,
    status_hint INTEGER NOT NULL DEFAULT 0,
    monitor INTEGER NOT NULL DEFAULT 0,
    monitor_mode INTEGER NOT NULL DEFAULT 0,
    onreboot INTEGER NOT NULL DEFAULT 0,
    pending_action INTEGER NOT NULL DEFAULT 0,
    every TEXT,
    collected_at REAL NOT NULL DEFAULT 0,
    data TEXT NOT NULL DEFAULT '{}',
    state TEXT NOT NULL DEFAULT 'init',
    state_since INTEGER,
    last_seen INTEGER NOT NULL,
    UNIQUE (host_id, name)
);

CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    host_id INTEGER REFERENCES hosts(id) ON DELETE CASCADE,
    service TEXT,
    service_type INTEGER,
    event_type INTEGER NOT NULL DEFAULT 0,
    state INTEGER NOT NULL DEFAULT 0,
    action INTEGER NOT NULL DEFAULT 0,
    message TEXT NOT NULL DEFAULT '',
    created_at REAL NOT NULL,
    source TEXT NOT NULL DEFAULT 'monit',
    acked_by TEXT,
    acked_at INTEGER
);
CREATE INDEX events_created ON events (created_at DESC);
CREATE INDEX events_host ON events (host_id, created_at DESC);
CREATE INDEX events_host_service ON events (host_id, service, created_at DESC);

CREATE TABLE series (
    id INTEGER PRIMARY KEY,
    host_id INTEGER NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    service TEXT NOT NULL,
    metric TEXT NOT NULL,
    UNIQUE (host_id, service, metric)
);

CREATE TABLE samples (
    series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    ts INTEGER NOT NULL,
    value REAL NOT NULL,
    PRIMARY KEY (series_id, ts)
) WITHOUT ROWID;
CREATE INDEX samples_ts ON samples (ts);

CREATE TABLE rollup_5m (
    series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    bucket INTEGER NOT NULL,
    sum REAL NOT NULL,
    count INTEGER NOT NULL,
    min REAL NOT NULL,
    max REAL NOT NULL,
    PRIMARY KEY (series_id, bucket)
) WITHOUT ROWID;
CREATE INDEX rollup_5m_bucket ON rollup_5m (bucket);

CREATE TABLE rollup_1h (
    series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    bucket INTEGER NOT NULL,
    sum REAL NOT NULL,
    count INTEGER NOT NULL,
    min REAL NOT NULL,
    max REAL NOT NULL,
    PRIMARY KEY (series_id, bucket)
) WITHOUT ROWID;
CREATE INDEX rollup_1h_bucket ON rollup_1h (bucket);

CREATE TABLE channels (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    config TEXT NOT NULL DEFAULT '{}',
    filter TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    last_status TEXT,
    last_sent_at INTEGER
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
