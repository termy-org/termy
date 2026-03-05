CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE theme (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    latest_version TEXT,
    file_key TEXT,
    github_username_claim TEXT NOT NULL,
    is_public BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE theme_version (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    theme_id UUID NOT NULL REFERENCES theme(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    file_key TEXT NOT NULL,
    changelog TEXT NOT NULL DEFAULT '',
    checksum_sha256 TEXT,
    created_by TEXT,
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(theme_id, version)
);

CREATE INDEX theme_slug_idx ON theme(slug);
CREATE INDEX theme_version_theme_id_idx ON theme_version(theme_id);
CREATE INDEX theme_version_theme_id_published_at_idx
    ON theme_version(theme_id, published_at DESC);
