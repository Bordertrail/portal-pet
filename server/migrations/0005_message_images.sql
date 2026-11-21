-- Text messages keep these columns NULL. Image data lives in a private OSS bucket.
ALTER TABLE messages ADD COLUMN image_key TEXT;
ALTER TABLE messages ADD COLUMN image_type TEXT;
ALTER TABLE messages ADD COLUMN image_width INTEGER;
ALTER TABLE messages ADD COLUMN image_height INTEGER;
ALTER TABLE messages ADD COLUMN image_bytes INTEGER;
