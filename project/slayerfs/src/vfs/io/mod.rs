use crate::chuck::{ChunkLayout, ChunkSpan, ChunkTag};

pub(crate) mod reader;
pub(crate) mod writer;

pub(crate) use reader::DataReader;
pub(crate) use reader::FileReader;
pub(crate) use writer::DataWriter;
pub(crate) use writer::FileWriter;

pub(crate) fn split_chunk_spans(layout: ChunkLayout, offset: u64, len: usize) -> Vec<ChunkSpan> {
    if len == 0 {
        return Vec::new();
    }

    let chunk_span = ChunkSpan::new(
        layout.chunk_index_of(offset),
        layout.within_chunk_offset(offset),
        len as u64,
    );
    chunk_span
        .split_into::<ChunkTag>(layout.chunk_size, layout.chunk_size, false)
        .collect()
}
