use crate::error::{AppError, BusinessError, MapToAppError};
use rand::RngCore;

pub fn hash_password(salt: &str, password: &str) -> Result<String, AppError> {
    Ok(bcrypt::hash_with_salt(
        password,
        bcrypt::DEFAULT_COST,
        salt.as_bytes().try_into().unwrap(),
    )
    .map_to_internal()?
    .to_string())
}

fn gen_random_string(size: usize) -> String {
    let mut rand = rand::rng();
    let mut dest = vec![0; size / 2];

    rand.fill_bytes(&mut dest);
    hex::encode(dest)
}

pub fn gen_salt() -> String {
    gen_random_string(16)
}

pub fn gen_password() -> String {
    gen_random_string(64)
}

pub fn check_password(salt: &str, expected: &str, actual: &str) -> Result<(), AppError> {
    let hash = hash_password(salt, actual)?;
    if hash == expected {
        return Ok(());
    }
    Err(BusinessError::BadRequest("invalid password".to_string()).into())
}
