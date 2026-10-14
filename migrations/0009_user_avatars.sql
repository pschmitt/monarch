-- Profile pictures, uploaded by the users themselves (already cropped and
-- scaled down by the browser). Served from /api/users/:id/avatar.
CREATE TABLE user_avatars (
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    mime TEXT NOT NULL,
    data BLOB NOT NULL,
    updated_at INTEGER NOT NULL
);
