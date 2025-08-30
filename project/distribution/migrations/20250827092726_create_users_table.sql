-- Add migration script here
CREATE TABLE users (
    id CHAR(32) PRIMARY KEY NOT NULL,
    username VARCHAR(255) NOT NULL UNIQUE,
    password TEXT NOT NULL
);

CREATE INDEX idx_users_username ON users (username);