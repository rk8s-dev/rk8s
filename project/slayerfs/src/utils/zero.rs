use bytes::Bytes;
use std::sync::LazyLock;

/// Preallocated zero range to build zero padding without repeated allocations.
pub(crate) static ZEROS: LazyLock<Bytes> =
    LazyLock::new(|| Bytes::from(vec![0_u8; 4 * 1024 * 1024]));

pub(crate) fn make_zero_bytes(mut len: usize) -> Vec<Bytes> {
    let mut result = Vec::new();
    let size = ZEROS.len();

    while len >= size {
        result.push(ZEROS.clone());
        len -= size;
    }

    if len > 0 {
        result.push(ZEROS.slice(0..len));
    }
    result
}
