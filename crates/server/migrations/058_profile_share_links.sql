-- Shared profiles (docs/profiles.md, "Sharing a profile"): a public page per
-- profile at /P/<code>, with the profile as a file to import (Roastwarden JSON, Artisan .alog).
-- The code is 25 base32 characters like a roast's share link (033); revoking one makes it a
-- 404 like an unknown code. Opens and downloads are counted without addresses.
CREATE TABLE profile_share_links (
    code TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL REFERENCES roast_profiles (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    revoked_at TEXT,
    views INTEGER NOT NULL DEFAULT 0,
    downloads INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT
);
CREATE INDEX profile_share_links_profile ON profile_share_links (profile_id);
