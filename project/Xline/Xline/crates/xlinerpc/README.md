# xlinerpc

Lightweight RPC type abstractions for Xline. The crate provides the foundational
types used to encode, transmit, and decode RPC messages over the wire.

## Core types

| Type | Description |
|------|-------------|
| `Status` | Simplified gRPC-compatible status with [`Code`] variants and convenience constructors (`Status::not_found(…)`, `Status::permission_denied(…)`, etc.) |
| `MetaData` | Binary key-value bag attached to every `Request`/`Response` envelope (analogous to HTTP headers) |
| `Request<T>` | Typed request envelope wrapping a protobuf payload `T` together with `MetaData` |
| `Response<T>` | Typed response envelope wrapping a protobuf payload `T` together with `MetaData` |
| `BinaryCodec` | Default length-prefixed binary codec used to serialize/deserialize envelopes on the wire |
| `Codec` trait | Extension point — implement this trait to plug in a custom serialization strategy |

## Wire encoding

`Request<T>` and `Response<T>` are serialized with `encode_to_vec()` / `encode_with(&codec)` and
deserialized with `decode_from_slice(&bytes)` / `decode_with(&bytes, &codec)`.  The default
`BinaryCodec` frame layout is:

```text
[meta_len: u32 BE] [metadata bytes] [protobuf payload bytes]
```

Metadata is encoded as `[count: u8] ([key_len: u16 BE] [key] [value_len: u16 BE] [value])…`.

## Status codes

`Status` mirrors the [gRPC status code specification](https://github.com/grpc/grpc/blob/master/doc/statuscodes.md).
Every `Code` variant has a matching constructor on `Status` so callers never need to write
`Status::new(Code::PermissionDenied, "…")` by hand.
