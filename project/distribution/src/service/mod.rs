use crate::error::{AppError, BusinessError, MapToAppError};

pub mod blob;
pub mod manifest;
pub mod repo;
pub mod user;

fn hash_password(salt: &str, password: &str) -> Result<String, AppError> {
    Ok(bcrypt::hash_with_salt(
        password,
        bcrypt::DEFAULT_COST,
        salt.as_bytes().try_into().unwrap(),
    )
    .map_to_internal()?
    .to_string())
}

pub fn check_password(salt: &str, expected: &str, actual: &str) -> Result<(), AppError> {
    let hash = hash_password(salt, actual)?;
    if hash == expected {
        return Ok(());
    }
    Err(BusinessError::BadRequest("invalid password".to_string()).into())
}
