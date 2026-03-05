ALTER TABLE theme
    ADD COLUMN IF NOT EXISTS github_user_id_claim BIGINT;

CREATE TABLE IF NOT EXISTS user_account (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    github_user_id BIGINT NOT NULL UNIQUE,
    github_login TEXT NOT NULL UNIQUE,
    avatar_url TEXT,
    name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS oauth_state (
    state TEXT PRIMARY KEY,
    redirect_to TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_session (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES user_account(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS theme_github_user_id_claim_idx ON theme(github_user_id_claim);
CREATE INDEX IF NOT EXISTS user_session_user_id_idx ON user_session(user_id);
CREATE INDEX IF NOT EXISTS user_session_expires_at_idx ON user_session(expires_at);
CREATE INDEX IF NOT EXISTS oauth_state_expires_at_idx ON oauth_state(expires_at);
