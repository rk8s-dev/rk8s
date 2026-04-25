CREATE TABLE repo_tags (
    repo_id UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    tag VARCHAR(255) NOT NULL,
    manifest_digest VARCHAR(255) NOT NULL,
    manifest_size_bytes BIGINT NOT NULL CHECK (manifest_size_bytes >= 0),
    pushed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (repo_id, tag)
);

CREATE INDEX idx_repo_tags_repo_id_pushed_at_desc ON repo_tags (repo_id, pushed_at DESC);
