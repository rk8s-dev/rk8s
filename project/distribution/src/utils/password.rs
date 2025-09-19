use crate::error::{AppError, BusinessError, MapToAppError};
use rand::RngCore;

pub fn hash_password(salt: impl AsRef<str>, password: impl AsRef<str>) -> Result<String, AppError> {
    Ok(bcrypt::hash_with_salt(
        password.as_ref(),
        bcrypt::DEFAULT_COST,
        salt.as_ref().as_bytes().try_into().unwrap(),
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

pub fn check_password(
    salt: impl AsRef<str>,
    expected: impl AsRef<str>,
    actual: impl AsRef<str>,
) -> Result<(), AppError> {
    let hash = hash_password(salt, actual)?;
    if hash == expected.as_ref() {
        return Ok(());
    }
    Err(BusinessError::BadRequest("invalid password".to_string()).into())
}
