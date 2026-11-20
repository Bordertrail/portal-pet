CREATE TABLE members (
    id INTEGER PRIMARY KEY,
    nickname TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1))
);

CREATE TABLE invitations (
    member_id INTEGER PRIMARY KEY REFERENCES members(id),
    code_hash TEXT NOT NULL UNIQUE
);

CREATE TABLE sessions (
    member_id INTEGER PRIMARY KEY REFERENCES members(id),
    token_hash TEXT NOT NULL UNIQUE
);

CREATE TABLE relationships (
    member_low INTEGER NOT NULL REFERENCES members(id),
    member_high INTEGER NOT NULL REFERENCES members(id),
    requester_id INTEGER NOT NULL REFERENCES members(id),
    accepted INTEGER NOT NULL DEFAULT 0 CHECK (accepted IN (0, 1)),
    PRIMARY KEY (member_low, member_high),
    CHECK (member_low < member_high),
    CHECK (requester_id IN (member_low, member_high))
);

CREATE INDEX relationships_by_high ON relationships(member_high);

CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sender_id INTEGER NOT NULL REFERENCES members(id),
    recipient_id INTEGER NOT NULL REFERENCES members(id),
    client_id TEXT NOT NULL,
    body TEXT NOT NULL,
    sent_at INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (sender_id, client_id),
    CHECK (sender_id <> recipient_id)
);

CREATE INDEX messages_by_conversation ON messages(sender_id, recipient_id, id);
