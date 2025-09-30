// @generated automatically by Diesel CLI.

diesel::table! {
    vault (vault_key) {
        vault_key -> Varchar,
        vault_value -> Varbinary,
    }
}
