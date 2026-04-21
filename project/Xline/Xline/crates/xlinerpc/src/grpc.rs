//! Shared gRPC-over-HTTP/3 helpers.
//!
//! This module centralizes frame encoding/decoding and grpc-status/grpc-message
//! header conversions so client and server keep identical wire semantics.

use bytes::{Bytes, BytesMut};
use http::{HeaderMap, HeaderValue, header::HeaderName};
use prost::Message;

use crate::{Code, Status};

/// gRPC frame header size: 1-byte compression flag + 4-byte message length.
pub const HEADER_SIZE: usize = 5;

const GRPC_STATUS_HEADER: HeaderName = HeaderName::from_static("grpc-status");
const GRPC_MESSAGE_HEADER: HeaderName = HeaderName::from_static("grpc-message");

/// Calculate the total frame size from a buffer containing a gRPC frame header.
///
/// Returns `(message_len, total_len)`.
#[must_use]
#[inline]
pub fn calculate_frame_size(buf: &[u8]) -> (usize, usize) {
    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    (len, HEADER_SIZE + len)
}

/// Percent-encode grpc-message header values.
#[must_use]
pub fn encode_message_header(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for b in message.bytes() {
        if (0x20..=0x7e).contains(&b) && b != b'%' {
            out.push(char::from(b));
        } else {
            out.push('%');
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{b:02X}"));
        }
    }
    out
}

/// Decode percent-encoded grpc-message header values.
#[must_use]
pub fn decode_message_header(message: &str) -> String {
    let bytes = message.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1] as char;
            let h2 = bytes[i + 2] as char;
            if let (Some(hi), Some(lo)) = (h1.to_digit(16), h2.to_digit(16)) {
                out.push(((hi << 4) as u8) | (lo as u8));
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Encode a protobuf message into a gRPC framed bytes buffer.
pub fn frame_encode<M: Message>(msg: &M) -> Result<Bytes, Status> {
    let body = msg.encode_to_vec();
    let len = u32::try_from(body.len()).map_err(|_| Status::internal("gRPC message too large"))?;
    let mut buf = BytesMut::with_capacity(HEADER_SIZE + body.len());
    buf.extend_from_slice(&[0u8]); // uncompressed
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&body);
    Ok(buf.freeze())
}

/// Decode one gRPC frame into a protobuf message.
///
/// Returns the decoded message and consumed byte count.
pub fn frame_decode<M: Message + Default>(buf: &[u8]) -> Result<(M, usize), Status> {
    if buf.len() < HEADER_SIZE {
        return Err(Status::internal("gRPC frame header too short"));
    }
    if buf[0] != 0 {
        return Err(Status::internal(
            "compressed gRPC frames are not supported (compression flag set)",
        ));
    }

    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    let total = HEADER_SIZE + len;
    if buf.len() < total {
        return Err(Status::internal("gRPC frame body too short"));
    }

    let msg = M::decode(&buf[HEADER_SIZE..total])
        .map_err(|e| Status::internal(format!("protobuf decode error: {e}")))?;
    Ok((msg, total))
}

/// Decode all contiguous gRPC frames from a byte buffer.
pub fn decode_all_frames<M: Message + Default>(buf: &[u8]) -> Result<Vec<M>, Status> {
    let mut msgs = Vec::new();
    let mut offset = 0;
    while offset < buf.len() {
        let (msg, consumed) = frame_decode::<M>(&buf[offset..])?;
        msgs.push(msg);
        offset += consumed;
    }
    Ok(msgs)
}

/// Extract a non-OK gRPC status from response headers/trailers.
#[must_use]
pub fn error_from_headers(headers: &HeaderMap) -> Option<Status> {
    let raw = headers.get(GRPC_STATUS_HEADER)?;
    let code = raw
        .to_str()
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(2);
    if code == 0 {
        return None;
    }

    let msg = headers
        .get(GRPC_MESSAGE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(decode_message_header)
        .unwrap_or_else(|| "unknown".to_string());
    Some(Status::new(Code::from(code), msg))
}

/// Build grpc-status/grpc-message trailers from an optional status.
#[must_use]
pub fn trailers_from_status(status: Option<&Status>) -> HeaderMap {
    let mut trailers = HeaderMap::new();

    let code = status
        .map(|s| i32::from(s.code()).to_string())
        .unwrap_or_else(|| "0".to_string());
    let status_value =
        HeaderValue::from_str(&code).unwrap_or_else(|_| HeaderValue::from_static("2"));
    let _ = trailers.insert(GRPC_STATUS_HEADER, status_value);

    if let Some(status) = status {
        if !status.message().is_empty() {
            let encoded = encode_message_header(status.message());
            let message_value = HeaderValue::from_str(&encoded)
                .unwrap_or_else(|_| HeaderValue::from_static("rpc error"));
            let _ = trailers.insert(GRPC_MESSAGE_HEADER, message_value);
        }
    }

    trailers
}
