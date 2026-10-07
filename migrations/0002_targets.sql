CREATE TABLE targets (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    username TEXT,
    password TEXT,
    ssh_destination TEXT,
    ssh_port INTEGER,
    interval INTEGER NOT NULL DEFAULT 30,
    tls_skip_verify INTEGER NOT NULL DEFAULT 1,
    enabled INTEGER NOT NULL DEFAULT 1,
    managed INTEGER NOT NULL DEFAULT 0,
    host_id INTEGER REFERENCES hosts(id) ON DELETE SET NULL,
    last_status TEXT,
    last_polled_at INTEGER,
    created_at INTEGER NOT NULL
);

ALTER TABLE hosts ADD COLUMN target_id INTEGER REFERENCES targets(id) ON DELETE SET NULL;
ALTER TABLE hosts ADD COLUMN last_push INTEGER;
