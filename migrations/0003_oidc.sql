ALTER TABLE users ADD COLUMN auth_source TEXT NOT NULL DEFAULT 'local';
ALTER TABLE users ADD COLUMN oidc_subject TEXT;
CREATE UNIQUE INDEX users_oidc_subject ON users (oidc_subject) WHERE oidc_subject IS NOT NULL;

CREATE TABLE oidc_states (
    state TEXT PRIMARY KEY,
    verifier TEXT NOT NULL,
    nonce TEXT NOT NULL,
    next TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
