//! Shared envelope type for RPC requests and responses.
//!
//! [`Envelope<T, Kind>`] is the single implementation backing both
//! [`Request<T>`](crate::Request) and [`Response<T>`](crate::Response).
//! The phantom `Kind` parameter keeps the two as distinct types at the
//! call site while sharing all logic here.

use std::marker::PhantomData;

use crate::{
    MetaData,
    codec::{BinaryCodec, Codec, DecodeError, EncodeError},
};

/// Generic RPC envelope: carries a protobuf payload together with binary metadata.
///
/// Use the type aliases [`crate::Request`] and [`crate::Response`] rather than
/// this type directly.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope<T, Kind> {
    /// The protobuf payload
    data: T,
    /// Binary metadata (headers, auth tokens, status info, …)
    meta: MetaData,
    /// Zero-size marker distinguishing Request from Response
    _kind: PhantomData<Kind>,
}

impl<T, Kind> Envelope<T, Kind> {
    /// Create a new envelope with data and metadata
    #[must_use]
    #[inline]
    pub fn new(data: T, meta: MetaData) -> Self {
        Self {
            data,
            meta,
            _kind: PhantomData,
        }
    }

    /// Create a new envelope with only data (empty metadata)
    #[must_use]
    #[inline]
    pub fn from_data(data: T) -> Self {
        Self {
            data,
            meta: MetaData::new(),
            _kind: PhantomData,
        }
    }

    /// Get a reference to the payload
    #[must_use]
    #[inline]
    pub fn data(&self) -> &T {
        &self.data
    }

    /// Tonic-compatible accessor for payload reference.
    #[must_use]
    #[inline]
    pub fn get_ref(&self) -> &T {
        &self.data
    }

    /// Get a mutable reference to the payload
    #[inline]
    pub fn data_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// Tonic-compatible accessor for mutable payload reference.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// Get a reference to the metadata
    #[must_use]
    #[inline]
    pub fn meta(&self) -> &MetaData {
        &self.meta
    }

    /// Tonic-compatible accessor for metadata.
    #[must_use]
    #[inline]
    pub fn metadata(&self) -> &MetaData {
        &self.meta
    }

    /// Get a mutable reference to the metadata
    #[inline]
    pub fn meta_mut(&mut self) -> &mut MetaData {
        &mut self.meta
    }

    /// Tonic-compatible mutable accessor for metadata.
    #[inline]
    pub fn metadata_mut(&mut self) -> &mut MetaData {
        &mut self.meta
    }

    /// Decompose into payload and metadata
    #[must_use]
    #[inline]
    pub fn into_parts(self) -> (T, MetaData) {
        (self.data, self.meta)
    }

    /// Tonic-compatible payload extractor.
    #[must_use]
    #[inline]
    pub fn into_inner(self) -> T {
        self.data
    }

    /// Encode to bytes using the default [`BinaryCodec`]
    ///
    /// # Errors
    /// Returns error if encoding fails
    #[inline]
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, EncodeError>
    where
        T: prost::Message,
    {
        self.encode_with(&BinaryCodec)
    }

    /// Encode to bytes using a custom codec
    ///
    /// # Errors
    /// Returns error if encoding fails
    pub fn encode_with<C: Codec>(&self, codec: &C) -> Result<Vec<u8>, EncodeError>
    where
        T: prost::Message,
    {
        codec.encode(&self.data, &self.meta)
    }

    /// Decode from bytes using the default [`BinaryCodec`]
    ///
    /// # Errors
    /// Returns error if decoding fails
    #[inline]
    pub fn decode_from_slice(bytes: &[u8]) -> Result<Self, DecodeError>
    where
        T: prost::Message + Default,
    {
        Self::decode_with(bytes, &BinaryCodec)
    }

    /// Decode from bytes using a custom codec
    ///
    /// # Errors
    /// Returns error if decoding fails
    pub fn decode_with<C: Codec>(bytes: &[u8], codec: &C) -> Result<Self, DecodeError>
    where
        T: prost::Message + Default,
    {
        let (data, meta) = codec.decode(bytes)?;
        Ok(Self {
            data,
            meta,
            _kind: PhantomData,
        })
    }
}
