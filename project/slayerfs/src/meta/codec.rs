use crate::meta::store::MetaError;

type CodecError = rkyv::rancor::Error;

#[inline]
fn serialize_err<E: std::fmt::Display>(e: E) -> MetaError {
    MetaError::Internal(format!("Serialization error: {e}"))
}

pub fn encode<T>(value: &T) -> Result<Vec<u8>, MetaError>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Portable,
{
    rkyv::to_bytes::<CodecError>(value)
        .map(|bytes| bytes.into_vec())
        .map_err(serialize_err)
}

pub fn decode<T>(bytes: &[u8]) -> Result<T, MetaError>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Portable,
{
    let archived = rkyv::access::<T::Archived, CodecError>(bytes).map_err(serialize_err)?;
    rkyv::deserialize::<T, CodecError>(archived).map_err(serialize_err)
}
