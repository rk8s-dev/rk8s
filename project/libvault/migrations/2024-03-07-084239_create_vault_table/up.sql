-- Create table vault
CREATE TABLE IF NOT EXISTS `vault` (
    `vault_key` varbinary(3072) NOT NULL,
    `vault_value` mediumblob,
    PRIMARY KEY (`vault_key`)
);
