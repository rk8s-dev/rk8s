-- Add migration script here

CREATE TABLE repos (
    id UUID PRIMARY KEY NOT NULL DEFAULT uuid_generate_v4(),
    namespace VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    last_pushed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(namespace, name)
);

DROP TRIGGER IF EXISTS set_timestamp ON repos;
CREATE TRIGGER set_timestamp
    BEFORE UPDATE ON repos
    FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();
