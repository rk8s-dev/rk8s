use crate::chuck::{ChunkLayout, ChunkSpan, ChunkTag};

mod reader;
mod writer;

pub use reader::DataReader;
pub use writer::DataWriter;

pub(crate) fn split_chunk_spans(layout: ChunkLayout, offset: u64, len: usize) -> Vec<ChunkSpan> {
    if len == 0 {
        return Vec::new();
    }

    let chunk_span = ChunkSpan::new(
        layout.chunk_index_of(offset),
        u32::try_from(layout.within_chunk_offset(offset))
            .expect("chunk offset must fit within u32 for spans"),
        u32::try_from(len).expect("length must fit within u32 for spans"),
    );
    chunk_span
        .split_into::<ChunkTag>(layout.chunk_size, layout.chunk_size, false)
        .collect()
}
