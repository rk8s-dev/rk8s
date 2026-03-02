//! Frame codec for QUIC transport
//!
//! This module implements the binary frame protocol for QUIC streams.
//! Each bidirectional stream carries one RPC call with the following format:
//!
//! Request header (client → server):
//! ```text
//! [2 bytes] method_id (big-endian u16, see `MethodId` enum)
//! [2 bytes] metadata entry count
//! For each entry: [2B key_len][key][2B val_len][val]
//! ```
//!
//! Frame types (after request header):
//! ```text
//! [1 byte] frame_type:
//!   0x01 = DATA   → [4 bytes length] + [N bytes protobuf data]
//!   0x02 = END    → no payload (stream end marker)
//!   0x03 = STATUS → [1 byte status_code] + [4 bytes details_len] + [N bytes CurpErrorWrapper]
//! ```

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::rpc::CurpError;

/// Maximum frame length: 16 MB
const MAX_FRAME_LEN: u32 = 16 * 1024 * 1024;

/// Maximum metadata entries: 64
const MAX_METADATA_ENTRIES: u16 = 64;

/// Maximum metadata key/value length: 4 KB
const MAX_METADATA_KV_LEN: u16 = 4 * 1024;

// ============================================================================
// MethodId — single-source definition via macro
// ============================================================================

macro_rules! define_method_ids {
    ( $( $(#[$meta:meta])* $variant:ident = $id:expr, $display:expr; )* ) => {
        /// Numeric RPC method identifier for QUIC transport.
        ///
        /// Replaces gRPC-style string paths with compact u16 IDs.
        /// Client and server must use the same mapping (guaranteed by this
        /// single macro definition).
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u16)]
        pub enum MethodId {
            $( $(#[$meta])* $variant = $id, )*
        }

        impl MethodId {
            /// Decode from raw u16. Returns `None` for unknown IDs.
            pub fn from_u16(v: u16) -> Option<Self> {
                match v {
                    $( $id => Some(Self::$variant), )*
                    _ => None,
                }
            }

            /// Encode to u16.
            #[inline]
            pub fn as_u16(self) -> u16 {
                self as u16
            }

            /// Human-readable name for logging/debugging.
            pub fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $display, )*
                }
            }
        }

        /// All defined method IDs (for testing completeness).
        /// Not part of the stable public API.
        #[doc(hidden)]
        #[cfg(any(test, feature = "quic-test"))]
        pub const ALL_METHOD_IDS: &[MethodId] = &[
            $( MethodId::$variant, )*
        ];
    };
}

define_method_ids! {
    // Protocol service (0x00xx)
    /// Fetch cluster membership information
    FetchCluster       = 0x0001, "FetchCluster";
    /// Fetch read state for linearizable reads
    FetchReadState     = 0x0002, "FetchReadState";
    /// Record a command proposal
    Record             = 0x0003, "Record";
    /// Read index for linearizable reads
    ReadIndex          = 0x0004, "ReadIndex";
    /// Shutdown the cluster
    Shutdown           = 0x0005, "Shutdown";
    /// Propose a configuration change
    ProposeConfChange  = 0x0006, "ProposeConfChange";
    /// Publish node information
    Publish            = 0x0007, "Publish";
    /// Move leader to another node
    MoveLeader         = 0x0008, "MoveLeader";
    /// Propose a command (server-streaming response)
    ProposeStream      = 0x0009, "ProposeStream";
    /// Lease keep-alive (client-streaming)
    LeaseKeepAlive     = 0x000A, "LeaseKeepAlive";

    // InnerProtocol service (0x01xx)
    /// Append entries (Raft replication)
    AppendEntries      = 0x0101, "AppendEntries";
    /// Vote (Raft leader election)
    Vote               = 0x0102, "Vote";
    /// Install snapshot (Raft state transfer, client-streaming)
    InstallSnapshot    = 0x0103, "InstallSnapshot";
    /// Trigger shutdown on a follower
    TriggerShutdown    = 0x0104, "TriggerShutdown";
    /// Try to become leader immediately
    TryBecomeLeaderNow = 0x0105, "TryBecomeLeaderNow";
}

/// Frame type constants
const FRAME_TYPE_DATA: u8 = 0x01;
const FRAME_TYPE_END: u8 = 0x02;
const FRAME_TYPE_STATUS: u8 = 0x03;

/// Status code constants
const STATUS_OK: u8 = 0x00;
const STATUS_ERROR: u8 = 0x01;

/// Frame types for QUIC protocol
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Frame {
    /// DATA frame containing protobuf-encoded message
    Data(Vec<u8>),
    /// END frame marking stream completion (application layer)
    End,
    /// STATUS frame with status code and optional error details
    Status {
        /// Status code (0x00 = OK, 0x01 = Error)
        code: u8,
        /// Error details (CurpErrorWrapper protobuf if code != 0)
        details: Vec<u8>,
    },
}

/// Stream state machine for frame sequence validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Some variants used only in tests or future features
pub(crate) enum StreamState {
    /// Expecting DATA frame (unary request/response first frame)
    ExpectData,
    /// Expecting STATUS frame only (unary response after receiving DATA)
    ExpectStatusOnly,
    /// Expecting END frame only (unary request after receiving DATA)
    ExpectEndOnly,
    /// Expecting DATA or STATUS frame (server-streaming response)
    ExpectDataOrStatus,
    /// Expecting DATA or END frame (client-streaming request)
    ExpectDataOrEnd,
    /// Terminal state, no more frames expected
    Terminal,
}

/// Stateful frame reader with stream state machine
pub(crate) struct FrameReader<R: AsyncRead + Unpin> {
    /// Underlying reader
    reader: R,
    /// Current stream state
    state: StreamState,
    /// Whether this is a response reader (affects STATUS handling in ExpectData)
    is_response: bool,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    /// Create a unary response reader (initial state: ExpectData)
    #[inline]
    pub(crate) fn new_unary_response(reader: R) -> Self {
        Self {
            reader,
            state: StreamState::ExpectData,
            is_response: true,
        }
    }

    /// Create a server-streaming response reader (initial state: ExpectDataOrStatus)
    #[inline]
    pub(crate) fn new_server_streaming(reader: R) -> Self {
        Self {
            reader,
            state: StreamState::ExpectDataOrStatus,
            is_response: true,
        }
    }

    /// Create a client-streaming request reader (initial state: ExpectDataOrEnd)
    #[inline]
    #[allow(dead_code)] // Used in server-side handling
    pub(crate) fn new_client_streaming(reader: R) -> Self {
        Self {
            reader,
            state: StreamState::ExpectDataOrEnd,
            is_response: false,
        }
    }

    /// Create a unary request reader (initial state: ExpectData)
    #[inline]
    pub(crate) fn new_unary_request(reader: R) -> Self {
        Self {
            reader,
            state: StreamState::ExpectData,
            is_response: false,
        }
    }

    /// Read next frame with state machine validation
    pub(crate) async fn read_frame(&mut self) -> Result<Frame, CurpError> {
        if self.state == StreamState::Terminal {
            return Err(CurpError::internal(
                "protocol violation: frame received after terminal frame",
            ));
        }

        let frame_type = match self.reader.read_u8().await {
            Ok(ft) => ft,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(CurpError::RpcTransport(()));
            }
            Err(e) => {
                return Err(CurpError::internal(format!("read frame type error: {e}")));
            }
        };

        let frame = match frame_type {
            FRAME_TYPE_DATA => {
                let len = self.reader.read_u32().await.map_err(|e| {
                    CurpError::internal(format!("read DATA frame length error: {e}"))
                })?;
                if len > MAX_FRAME_LEN {
                    return Err(CurpError::internal("frame too large"));
                }
                let mut data = vec![0u8; len as usize];
                let _ = self.reader.read_exact(&mut data).await.map_err(|e| {
                    CurpError::internal(format!("read DATA frame payload error: {e}"))
                })?;
                Frame::Data(data)
            }
            FRAME_TYPE_END => Frame::End,
            FRAME_TYPE_STATUS => {
                let code = self
                    .reader
                    .read_u8()
                    .await
                    .map_err(|e| CurpError::internal(format!("read STATUS code error: {e}")))?;
                if code != STATUS_OK && code != STATUS_ERROR {
                    return Err(CurpError::internal(format!(
                        "protocol violation: unknown status code 0x{code:02X}"
                    )));
                }
                let details_len = self.reader.read_u32().await.map_err(|e| {
                    CurpError::internal(format!("read STATUS details length error: {e}"))
                })?;
                if details_len > MAX_FRAME_LEN {
                    return Err(CurpError::internal(
                        "protocol violation: STATUS details too large",
                    ));
                }
                let mut details = vec![0u8; details_len as usize];
                if details_len > 0 {
                    let _ = self.reader.read_exact(&mut details).await.map_err(|e| {
                        CurpError::internal(format!("read STATUS details error: {e}"))
                    })?;
                }
                Frame::Status { code, details }
            }
            _ => {
                return Err(CurpError::internal(format!(
                    "unknown frame type 0x{frame_type:02X}"
                )));
            }
        };

        // Validate and transition state
        self.validate_and_transition(&frame)?;

        Ok(frame)
    }

    /// Validate frame against current state and transition to next state
    fn validate_and_transition(&mut self, frame: &Frame) -> Result<(), CurpError> {
        match (self.state, frame) {
            // ExpectData state
            (StreamState::ExpectData, Frame::Data(_)) => {
                self.state = if self.is_response {
                    StreamState::ExpectStatusOnly
                } else {
                    StreamState::ExpectEndOnly
                };
            }
            (StreamState::ExpectData, Frame::Status { .. }) if self.is_response => {
                // Server returned error before sending data - valid for response
                self.state = StreamState::Terminal;
            }
            (StreamState::ExpectData, Frame::End) => {
                // Peer closed without sending data
                return Err(CurpError::RpcTransport(()));
            }
            (StreamState::ExpectData, _) => {
                return Err(CurpError::internal(format!(
                    "protocol violation: unexpected frame in state ExpectData"
                )));
            }

            // ExpectStatusOnly state
            (StreamState::ExpectStatusOnly, Frame::Status { .. }) => {
                self.state = StreamState::Terminal;
            }
            (StreamState::ExpectStatusOnly, Frame::Data(_)) => {
                return Err(CurpError::internal(
                    "protocol violation: unexpected DATA in state ExpectStatusOnly",
                ));
            }
            (StreamState::ExpectStatusOnly, _) => {
                return Err(CurpError::internal(format!(
                    "protocol violation: unexpected frame in state ExpectStatusOnly"
                )));
            }

            // ExpectEndOnly state
            (StreamState::ExpectEndOnly, Frame::End) => {
                self.state = StreamState::Terminal;
            }
            (StreamState::ExpectEndOnly, Frame::Data(_)) => {
                return Err(CurpError::internal(
                    "protocol violation: unexpected DATA in state ExpectEndOnly",
                ));
            }
            (StreamState::ExpectEndOnly, _) => {
                return Err(CurpError::internal(format!(
                    "protocol violation: unexpected frame in state ExpectEndOnly"
                )));
            }

            // ExpectDataOrStatus state (server-streaming)
            (StreamState::ExpectDataOrStatus, Frame::Data(_)) => {
                // Stay in same state, more data may come
            }
            (StreamState::ExpectDataOrStatus, Frame::Status { .. }) => {
                self.state = StreamState::Terminal;
            }
            (StreamState::ExpectDataOrStatus, Frame::End) => {
                return Err(CurpError::internal(
                    "protocol violation: unexpected END in state ExpectDataOrStatus",
                ));
            }

            // ExpectDataOrEnd state (client-streaming)
            (StreamState::ExpectDataOrEnd, Frame::Data(_)) => {
                // Stay in same state, more data may come
            }
            (StreamState::ExpectDataOrEnd, Frame::End) => {
                self.state = StreamState::Terminal;
            }
            (StreamState::ExpectDataOrEnd, Frame::Status { .. }) => {
                return Err(CurpError::internal(
                    "protocol violation: unexpected STATUS in state ExpectDataOrEnd",
                ));
            }

            // Terminal state - should not reach here due to early check
            (StreamState::Terminal, _) => {
                return Err(CurpError::internal(
                    "protocol violation: frame received after terminal frame",
                ));
            }
        }
        Ok(())
    }

    /// Check if reader is in terminal state
    #[inline]
    #[allow(dead_code)] // Used in tests
    pub(crate) fn is_terminal(&self) -> bool {
        self.state == StreamState::Terminal
    }
}

/// Frame writer (handles frame encoding only, not stream lifecycle)
pub(crate) struct FrameWriter<W: AsyncWrite + Unpin> {
    /// Underlying writer
    writer: W,
}

impl<W: AsyncWrite + Unpin> FrameWriter<W> {
    /// Create a new frame writer
    #[inline]
    pub(crate) fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Write request header (method ID + metadata)
    pub(crate) async fn write_request_header(
        &mut self,
        method: MethodId,
        meta: &[(String, String)],
    ) -> Result<(), CurpError> {
        self.writer
            .write_u16(method.as_u16())
            .await
            .map_err(|e| CurpError::internal(format!("write method id error: {e}")))?;
        self.write_metadata(meta).await
    }

    /// Write metadata entries (shared by `write_request_header` and test helper)
    async fn write_metadata(&mut self, meta: &[(String, String)]) -> Result<(), CurpError> {
        if meta.len() > MAX_METADATA_ENTRIES as usize {
            return Err(CurpError::internal("header too large: metadata entries"));
        }

        #[allow(clippy::cast_possible_truncation)]
        let meta_count = meta.len() as u16;
        self.writer
            .write_u16(meta_count)
            .await
            .map_err(|e| CurpError::internal(format!("write metadata count error: {e}")))?;

        for (key, value) in meta {
            let key_bytes = key.as_bytes();
            let value_bytes = value.as_bytes();
            if key_bytes.len() > MAX_METADATA_KV_LEN as usize
                || value_bytes.len() > MAX_METADATA_KV_LEN as usize
            {
                return Err(CurpError::internal("header too large: metadata kv"));
            }

            #[allow(clippy::cast_possible_truncation)]
            let key_len = key_bytes.len() as u16;
            #[allow(clippy::cast_possible_truncation)]
            let value_len = value_bytes.len() as u16;

            self.writer
                .write_u16(key_len)
                .await
                .map_err(|e| CurpError::internal(format!("write key length error: {e}")))?;
            self.writer
                .write_all(key_bytes)
                .await
                .map_err(|e| CurpError::internal(format!("write key error: {e}")))?;
            self.writer
                .write_u16(value_len)
                .await
                .map_err(|e| CurpError::internal(format!("write value length error: {e}")))?;
            self.writer
                .write_all(value_bytes)
                .await
                .map_err(|e| CurpError::internal(format!("write value error: {e}")))?;
        }

        Ok(())
    }

    /// Write a frame
    pub(crate) async fn write_frame(&mut self, frame: &Frame) -> Result<(), CurpError> {
        match *frame {
            Frame::Data(ref data) => {
                self.writer
                    .write_u8(FRAME_TYPE_DATA)
                    .await
                    .map_err(|e| CurpError::internal(format!("write DATA type error: {e}")))?;
                #[allow(clippy::cast_possible_truncation)]
                let len = data.len() as u32;
                self.writer
                    .write_u32(len)
                    .await
                    .map_err(|e| CurpError::internal(format!("write DATA length error: {e}")))?;
                self.writer
                    .write_all(data)
                    .await
                    .map_err(|e| CurpError::internal(format!("write DATA payload error: {e}")))?;
            }
            Frame::End => {
                self.writer
                    .write_u8(FRAME_TYPE_END)
                    .await
                    .map_err(|e| CurpError::internal(format!("write END error: {e}")))?;
            }
            Frame::Status { code, ref details } => {
                self.writer
                    .write_u8(FRAME_TYPE_STATUS)
                    .await
                    .map_err(|e| CurpError::internal(format!("write STATUS type error: {e}")))?;
                self.writer
                    .write_u8(code)
                    .await
                    .map_err(|e| CurpError::internal(format!("write STATUS code error: {e}")))?;
                #[allow(clippy::cast_possible_truncation)]
                let details_len = details.len() as u32;
                self.writer.write_u32(details_len).await.map_err(|e| {
                    CurpError::internal(format!("write STATUS details length error: {e}"))
                })?;
                if !details.is_empty() {
                    self.writer.write_all(details).await.map_err(|e| {
                        CurpError::internal(format!("write STATUS details error: {e}"))
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Flush buffered data to the underlying writer
    pub(crate) async fn flush(&mut self) -> Result<(), CurpError> {
        self.writer
            .flush()
            .await
            .map_err(|e| CurpError::internal(format!("flush error: {e}")))
    }

    /// Consume writer and return underlying stream
    #[inline]
    pub(crate) fn into_inner(self) -> W {
        self.writer
    }
}

/// Test-only: write a raw u16 method ID header (bypasses `MethodId` type safety).
#[doc(hidden)]
#[cfg(feature = "quic-test")]
impl<W: AsyncWrite + Unpin> FrameWriter<W> {
    /// Write a raw u16 method ID + metadata. Used to test unknown-method error path.
    #[doc(hidden)]
    #[allow(unreachable_pub)]
    pub async fn write_raw_method_header(
        &mut self,
        raw_method_id: u16,
        meta: &[(String, String)],
    ) -> Result<(), CurpError> {
        self.writer
            .write_u16(raw_method_id)
            .await
            .map_err(|e| CurpError::internal(format!("write method id error: {e}")))?;
        self.write_metadata(meta).await
    }
}

/// Read request header from stream.
///
/// Returns the raw method ID (u16) without validating it against `MethodId`.
/// The caller (`handle_stream`) is responsible for converting to `MethodId`
/// and returning a structured error for unknown IDs.
pub(crate) async fn read_request_header<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<(u16, Vec<(String, String)>), CurpError> {
    // Read method ID
    let method_raw = r
        .read_u16()
        .await
        .map_err(|e| CurpError::internal(format!("read method id error: {e}")))?;

    // Read metadata
    let meta_count = r
        .read_u16()
        .await
        .map_err(|e| CurpError::internal(format!("read metadata count error: {e}")))?;
    if meta_count > MAX_METADATA_ENTRIES {
        return Err(CurpError::internal("header too large: metadata entries"));
    }

    let mut meta = Vec::with_capacity(meta_count as usize);
    for _ in 0..meta_count {
        let key_len = r
            .read_u16()
            .await
            .map_err(|e| CurpError::internal(format!("read key length error: {e}")))?;
        if key_len > MAX_METADATA_KV_LEN {
            return Err(CurpError::internal("header too large: metadata key"));
        }
        let mut key_bytes = vec![0u8; key_len as usize];
        let _ = r
            .read_exact(&mut key_bytes)
            .await
            .map_err(|e| CurpError::internal(format!("read key error: {e}")))?;
        let key = String::from_utf8(key_bytes)
            .map_err(|e| CurpError::internal(format!("invalid key encoding: {e}")))?;

        let value_len = r
            .read_u16()
            .await
            .map_err(|e| CurpError::internal(format!("read value length error: {e}")))?;
        if value_len > MAX_METADATA_KV_LEN {
            return Err(CurpError::internal("header too large: metadata value"));
        }
        let mut value_bytes = vec![0u8; value_len as usize];
        let _ = r
            .read_exact(&mut value_bytes)
            .await
            .map_err(|e| CurpError::internal(format!("read value error: {e}")))?;
        let value = String::from_utf8(value_bytes)
            .map_err(|e| CurpError::internal(format!("invalid value encoding: {e}")))?;

        meta.push((key, value));
    }

    Ok((method_raw, meta))
}

/// Status code for OK response
#[inline]
pub(crate) const fn status_ok() -> u8 {
    STATUS_OK
}

/// Status code for error response
#[inline]
pub(crate) const fn status_error() -> u8 {
    STATUS_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_frame_roundtrip_data() {
        let (client, server) = tokio::io::duplex(1024);
        let (read_half, _write_half) = tokio::io::split(server);
        let (_, client_write) = tokio::io::split(client);

        let mut writer = FrameWriter::new(client_write);
        let data = b"hello world".to_vec();
        writer
            .write_frame(&Frame::Data(data.clone()))
            .await
            .unwrap();

        let mut reader = FrameReader::new_unary_response(read_half);
        let frame = reader.read_frame().await.unwrap();
        assert_eq!(frame, Frame::Data(data));
    }

    #[tokio::test]
    async fn test_frame_roundtrip_end() {
        let (client, server) = tokio::io::duplex(1024);
        let (read_half, _) = tokio::io::split(server);
        let (_, client_write) = tokio::io::split(client);

        let mut writer = FrameWriter::new(client_write);
        // First send DATA, then END for client-streaming
        writer
            .write_frame(&Frame::Data(vec![1, 2, 3]))
            .await
            .unwrap();
        writer.write_frame(&Frame::End).await.unwrap();

        let mut reader = FrameReader::new_client_streaming(read_half);
        let frame1 = reader.read_frame().await.unwrap();
        assert!(matches!(frame1, Frame::Data(_)));
        let frame2 = reader.read_frame().await.unwrap();
        assert_eq!(frame2, Frame::End);
        assert!(reader.is_terminal());
    }

    #[tokio::test]
    async fn test_frame_roundtrip_status() {
        let (client, server) = tokio::io::duplex(1024);
        let (read_half, _) = tokio::io::split(server);
        let (_, client_write) = tokio::io::split(client);

        let mut writer = FrameWriter::new(client_write);
        // For unary response: DATA then STATUS
        writer
            .write_frame(&Frame::Data(vec![1, 2, 3]))
            .await
            .unwrap();
        writer
            .write_frame(&Frame::Status {
                code: status_ok(),
                details: vec![],
            })
            .await
            .unwrap();

        let mut reader = FrameReader::new_unary_response(read_half);
        let frame1 = reader.read_frame().await.unwrap();
        assert!(matches!(frame1, Frame::Data(_)));
        let frame2 = reader.read_frame().await.unwrap();
        assert!(matches!(frame2, Frame::Status { code: 0, .. }));
        assert!(reader.is_terminal());
    }

    #[tokio::test]
    async fn test_request_header_roundtrip() {
        let (client, server) = tokio::io::duplex(1024);
        let (mut read_half, _) = tokio::io::split(server);
        let (_, client_write) = tokio::io::split(client);

        let mut writer = FrameWriter::new(client_write);
        let meta = vec![
            ("bypass".to_owned(), "true".to_owned()),
            ("token".to_owned(), "jwt123".to_owned()),
        ];
        writer
            .write_request_header(MethodId::FetchCluster, &meta)
            .await
            .unwrap();

        let (method_raw, read_meta) = read_request_header(&mut read_half).await.unwrap();
        assert_eq!(method_raw, MethodId::FetchCluster.as_u16());
        assert_eq!(read_meta, meta);
    }

    #[test]
    fn test_method_id_roundtrip() {
        for &method in ALL_METHOD_IDS {
            assert_eq!(
                MethodId::from_u16(method.as_u16()),
                Some(method),
                "roundtrip failed for {:?} (0x{:04X})",
                method,
                method.as_u16(),
            );
        }
    }

    #[test]
    fn test_method_id_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for &method in ALL_METHOD_IDS {
            assert!(
                seen.insert(method.as_u16()),
                "duplicate method ID: 0x{:04X} ({:?})",
                method.as_u16(),
                method,
            );
        }
        assert_eq!(seen.len(), ALL_METHOD_IDS.len());
    }

    #[test]
    fn test_method_id_unknown_returns_none() {
        assert_eq!(MethodId::from_u16(0x0000), None);
        assert_eq!(MethodId::from_u16(0xFFFF), None);
        assert_eq!(MethodId::from_u16(0x00FF), None);
    }

    #[tokio::test]
    async fn test_state_machine_unary_response() {
        let (client, server) = tokio::io::duplex(1024);
        let (read_half, _) = tokio::io::split(server);
        let (_, client_write) = tokio::io::split(client);

        let mut writer = FrameWriter::new(client_write);
        // Valid sequence: DATA -> STATUS
        writer.write_frame(&Frame::Data(vec![1])).await.unwrap();
        writer
            .write_frame(&Frame::Status {
                code: status_ok(),
                details: vec![],
            })
            .await
            .unwrap();

        let mut reader = FrameReader::new_unary_response(read_half);
        assert!(!reader.is_terminal());
        let _ = reader.read_frame().await.unwrap();
        assert!(!reader.is_terminal());
        let _ = reader.read_frame().await.unwrap();
        assert!(reader.is_terminal());
    }

    #[tokio::test]
    async fn test_state_machine_reject_double_data_in_unary() {
        let (client, server) = tokio::io::duplex(1024);
        let (read_half, _) = tokio::io::split(server);
        let (_, client_write) = tokio::io::split(client);

        let mut writer = FrameWriter::new(client_write);
        // Invalid: two DATA frames in unary response
        writer.write_frame(&Frame::Data(vec![1])).await.unwrap();
        writer.write_frame(&Frame::Data(vec![2])).await.unwrap();

        let mut reader = FrameReader::new_unary_response(read_half);
        let _ = reader.read_frame().await.unwrap(); // First DATA ok
        let result = reader.read_frame().await; // Second DATA should fail
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_server_streaming_multiple_data() {
        let (client, server) = tokio::io::duplex(1024);
        let (read_half, _) = tokio::io::split(server);
        let (_, client_write) = tokio::io::split(client);

        let mut writer = FrameWriter::new(client_write);
        // Valid: multiple DATA frames then STATUS
        writer.write_frame(&Frame::Data(vec![1])).await.unwrap();
        writer.write_frame(&Frame::Data(vec![2])).await.unwrap();
        writer.write_frame(&Frame::Data(vec![3])).await.unwrap();
        writer
            .write_frame(&Frame::Status {
                code: status_ok(),
                details: vec![],
            })
            .await
            .unwrap();

        let mut reader = FrameReader::new_server_streaming(read_half);
        for _ in 0..3 {
            let frame = reader.read_frame().await.unwrap();
            assert!(matches!(frame, Frame::Data(_)));
            assert!(!reader.is_terminal());
        }
        let frame = reader.read_frame().await.unwrap();
        assert!(matches!(frame, Frame::Status { .. }));
        assert!(reader.is_terminal());
    }

    /// Frozen mapping test: ensures MethodId numeric values never drift.
    /// If this test fails, you've accidentally renumbered a method — which
    /// breaks wire compatibility with older peers.
    #[test]
    fn test_method_id_frozen_mapping() {
        // Protocol service (0x00xx)
        assert_eq!(MethodId::FetchCluster.as_u16(), 0x0001);
        assert_eq!(MethodId::FetchReadState.as_u16(), 0x0002);
        assert_eq!(MethodId::Record.as_u16(), 0x0003);
        assert_eq!(MethodId::ReadIndex.as_u16(), 0x0004);
        assert_eq!(MethodId::Shutdown.as_u16(), 0x0005);
        assert_eq!(MethodId::ProposeConfChange.as_u16(), 0x0006);
        assert_eq!(MethodId::Publish.as_u16(), 0x0007);
        assert_eq!(MethodId::MoveLeader.as_u16(), 0x0008);
        assert_eq!(MethodId::ProposeStream.as_u16(), 0x0009);
        assert_eq!(MethodId::LeaseKeepAlive.as_u16(), 0x000A);

        // InnerProtocol service (0x01xx)
        assert_eq!(MethodId::AppendEntries.as_u16(), 0x0101);
        assert_eq!(MethodId::Vote.as_u16(), 0x0102);
        assert_eq!(MethodId::InstallSnapshot.as_u16(), 0x0103);
        assert_eq!(MethodId::TriggerShutdown.as_u16(), 0x0104);
        assert_eq!(MethodId::TryBecomeLeaderNow.as_u16(), 0x0105);

        // Total count guard: if you add a new method, update this assertion.
        assert_eq!(ALL_METHOD_IDS.len(), 15);
    }
}
