-- Add migration script here
CREATE TABLE repos (
    id CHAR(32) PRIMARY KEY NOT NULL,
    name VARCHAR(255) NOT NULL UNIQUE,
    is_public INTEGER NOT NULL DEFAULT 1,
)

CREATE INDEX idx_repos_name ON repos (name);