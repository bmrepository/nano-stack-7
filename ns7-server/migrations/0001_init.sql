-- Initial schema. Replaces the in-memory placeholder stores (workspaces,
-- devices, admin account, server Noise identity) that were lost on every
-- container restart — see README Section 8 for the intended data model.
--
-- IDs are TEXT rather than UUID, and findings are TEXT holding JSON, to
-- keep the sqlx feature set minimal (no uuid/json features needed).

-- Single-row table holding the server's long-term Noise static key. This
-- MUST persist: it's the responder identity for every enrollment and
-- check-in handshake, and the HMAC key for device certificates. Losing it
-- invalidates every enrolled device.
CREATE TABLE IF NOT EXISTS server_identity (
    id           INTEGER PRIMARY KEY,
    private_key  BYTEA   NOT NULL,
    CONSTRAINT server_identity_single_row CHECK (id = 1)
);

CREATE TABLE IF NOT EXISTS admin_accounts (
    username       TEXT PRIMARY KEY,
    password_hash  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workspaces (
    id               TEXT   PRIMARY KEY,
    name             TEXT   NOT NULL,
    created_at_unix  BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
    device_public_key    BYTEA  PRIMARY KEY,
    device_id            TEXT   NOT NULL,
    -- ON DELETE CASCADE implements the confirmed immediate-revocation
    -- policy for workspace deletion (README Section 10, decision 4).
    workspace_id         TEXT   NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    hostname             TEXT   NOT NULL,
    os_version           TEXT   NOT NULL,
    issued_at_unix       BIGINT NOT NULL,
    workspace_signature  BYTEA  NOT NULL,
    last_checkin_unix    BIGINT,
    last_findings        TEXT   NOT NULL DEFAULT '[]'
);
