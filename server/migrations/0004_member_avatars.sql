ALTER TABLE members ADD COLUMN avatar TEXT NOT NULL DEFAULT 'default'
    CHECK (avatar IN ('default', 'blue', 'purple'));
