-- Approvals (crates/server/src/auth/approvals.rs, docs/control.md "Approvals"): a token with
-- `require_approval` that starts a roast on a physical roaster or deletes a roast, bean, profile or
-- roaster gets 202 and a row here; the owner approves or denies it, and on approval the server
-- makes the original call as the token. Existing tokens keep working as before (0); Settings →
-- Tokens and the API default new tokens to 1.
ALTER TABLE api_tokens ADD COLUMN require_approval INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS approvals (
    -- Random and unguessable: it is in the link sent out
    id            TEXT PRIMARY KEY,
    token_id      INTEGER REFERENCES api_tokens(id) ON DELETE SET NULL,
    token_name    TEXT NOT NULL,
    -- roast.start | roast.delete | bean.delete | profile.delete | roaster.delete
    action        TEXT NOT NULL,
    method        TEXT NOT NULL,
    path          TEXT NOT NULL,
    -- The held call's body (JSON), made again on approval
    body          TEXT,
    target_id     TEXT NOT NULL,
    target_name   TEXT,
    device_id     TEXT,
    client_ip     TEXT,
    url           TEXT NOT NULL,
    -- pending | approved (running) | done | failed | denied; expiry is read from expires_at
    status        TEXT NOT NULL DEFAULT 'pending',
    created_at    TEXT NOT NULL,
    expires_at    TEXT NOT NULL,
    decided_at    TEXT,
    -- What the approved call answered: HTTP status and body (JSON)
    result_status INTEGER,
    result        TEXT
);
CREATE INDEX IF NOT EXISTS idx_approvals_created ON approvals(created_at DESC);
