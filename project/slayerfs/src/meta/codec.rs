use crate::meta::store::MetaError;

type CodecError = rkyv::rancor::Error;

#[inline]
fn serialize_err<E: std::fmt::Display>(e: E) -> MetaError {
    MetaError::Internal(format!("Serialization error: {e}"))
}

pub fn encode<T>(value: &T) -> Result<Vec<u8>, MetaError>
where
    T: rkyv::Archive + for<'a> rkyv::Serialize<rkyv::rancor::Strategy<rkyv::ser::Serializer<rkyv::util::AlignedVec, rkyv::ser::allocator::ArenaHandle<'a>, rkyv::ser::sharing::Share>, rkyv::rancor::Error>>,
    T::Archived: rkyv::Portable,
{
    rkyv::to_bytes::<CodecError>(value)
        .map(|bytes| bytes.into_vec())
        .map_err(serialize_err)
}

pub fn decode<T>(bytes: &[u8]) -> Result<T, MetaError>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::rancor::Strategy<rkyv::de::Pool, CodecError>>,
{
    // SAFETY: We assume the bytes are valid rkyv-encoded data for type T.
    // In practice, callers should only use this with types they know are properly serialized.
    let archived = unsafe {
        rkyv::access_unchecked::<T::Archived>(bytes)
    };
    rkyv::deserialize::<T, CodecError>(archived).map_err(serialize_err)
}
