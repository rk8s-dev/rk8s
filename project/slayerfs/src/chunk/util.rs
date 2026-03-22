//! Additional type aliases for `Span` specializations used around the chunk code.

use super::span::{ChunkTag, Span};

/// A range inside a chunk representing part of a file span.
pub type ChunkSpan = Span<ChunkTag>;
