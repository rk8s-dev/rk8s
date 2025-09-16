// Copyright 2023 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
//! Module containing versions of the standard library's [`Read`](std::io::Read) and
//! [`Write`](std::io::Write) traits compatible with volatile memory accesses.

use crate::bitmap::BitmapSlice;
use crate::volatile_memory::copy_slice_impl::{copy_from_volatile_slice, copy_to_volatile_slice};
use crate::{VolatileMemoryError, VolatileSlice};
use std::io::{Cursor, ErrorKind, Stdout};
use std::os::fd::AsRawFd;

/// A version of the standard library's [`Read`](std::io::Read) trait that operates on volatile
/// memory instead of slices
///
/// This trait is needed as rust slices (`&[u8]` and `&mut [u8]`) cannot be used when operating on
/// guest memory [1].
///
/// [1]: https://github.com/rust-vmm/vm-memory/pull/217
pub trait ReadVolatile {
    /// Tries to read some bytes into the given [`VolatileSlice`] buffer, returning how many bytes
    /// were read.
    ///
    /// The behavior of implementations should be identical to [`Read::read`](std::io::Read::read)
    fn read_volatile<B: BitmapSlice>(
        &mut self,
        buf: &mut VolatileSlice<B>,
    ) -> Result<usize, VolatileMemoryError>;

    /// Tries to fill the given [`VolatileSlice`] buffer by reading from `self` returning an error
    /// if insufficient bytes could be read.
    ///
    /// The default implementation is identical to that of [`Read::read_exact`](std::io::Read::read_exact)
    fn read_exact_volatile<B: BitmapSlice>(
        &mut self,
        buf: &mut VolatileSlice<B>,
    ) -> Result<(), VolatileMemoryError> {
        // Implementation based on https://github.com/rust-lang/rust/blob/7e7483d26e3cec7a44ef00cf7ae6c9c8c918bec6/library/std/src/io/mod.rs#L465

        let mut partial_buf = buf.offset(0)?;

        while !partial_buf.is_empty() {
            match self.read_volatile(&mut partial_buf) {
                Err(VolatileMemoryError::IOError(err)) if err.kind() == ErrorKind::Interrupted => {
                    continue
                }
                Ok(0) => {
                    return Err(VolatileMemoryError::IOError(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "failed to fill whole buffer",
                    )))
                }
                Ok(bytes_read) => partial_buf = partial_buf.offset(bytes_read)?,
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }
}

/// A version of the standard library's [`Write`](std::io::Write) trait that operates on volatile
/// memory instead of slices.
///
/// This trait is needed as rust slices (`&[u8]` and `&mut [u8]`) cannot be used when operating on
/// guest memory [1].
///
/// [1]: https://github.com/rust-vmm/vm-memory/pull/217
pub trait WriteVolatile {
    /// Tries to write some bytes from the given [`VolatileSlice`] buffer, returning how many bytes
    /// were written.
    ///
    /// The behavior of implementations should be identical to [`Write::write`](std::io::Write::write)
    fn write_volatile<B: BitmapSlice>(
        &mut self,
        buf: &VolatileSlice<B>,
    ) -> Result<usize, VolatileMemoryError>;

    /// Tries write the entire content of the given [`VolatileSlice`] buffer to `self` returning an
    /// error if not all bytes could be written.
    ///
    /// The default implementation is identical to that of [`Write::write_all`](std::io::Write::write_all)
    fn write_all_volatile<B: BitmapSlice>(
        &mut self,
        buf: &VolatileSlice<B>,
    ) -> Result<(), VolatileMemoryError> {
        // Based on https://github.com/rust-lang/rust/blob/7e7483d26e3cec7a44ef00cf7ae6c9c8c918bec6/library/std/src/io/mod.rs#L1570

        let mut partial_buf = buf.offset(0)?;

        while !partial_buf.is_empty() {
            match self.write_volatile(&partial_buf) {
                Err(VolatileMemoryError::IOError(err)) if err.kind() == ErrorKind::Interrupted => {
                    continue
                }
                Ok(0) => {
                    return Err(VolatileMemoryError::IOError(std::io::Error::new(
                        ErrorKind::WriteZero,
                        "failed to write whole buffer",
                    )))
                }
                Ok(bytes_written) => partial_buf = partial_buf.offset(bytes_written)?,
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }
}

// We explicitly implement our traits for [`std::fs::File`] and [`std::os::unix::net::UnixStream`]
// instead of providing blanket implementation for [`AsRawFd`] due to trait coherence limitations: A
// blanket implementation would prevent us from providing implementations for `&mut [u8]` below, as
// "an upstream crate could implement AsRawFd for &mut [u8]`.

macro_rules! impl_read_write_volatile_for_raw_fd {
    ($raw_fd_ty:ty) => {
        impl ReadVolatile for $raw_fd_ty {
            fn read_volatile<B: BitmapSlice>(
                &mut self,
                buf: &mut VolatileSlice<B>,
            ) -> Result<usize, VolatileMemoryError> {
                read_volatile_raw_fd(self, buf)
            }
        }

        impl WriteVolatile for $raw_fd_ty {
            fn write_volatile<B: BitmapSlice>(
                &mut self,
                buf: &VolatileSlice<B>,
            ) -> Result<usize, VolatileMemoryError> {
                write_volatile_raw_fd(self, buf)
            }
        }
    };
}

impl WriteVolatile for Stdout {
    fn write_volatile<B: BitmapSlice>(
        &mut self,
        buf: &VolatileSlice<B>,
    ) -> Result<usize, VolatileMemoryError> {
        write_volatile_raw_fd(self, buf)
    }
}

impl_read_write_volatile_for_raw_fd!(std::fs::File);
impl_read_write_volatile_for_raw_fd!(std::net::TcpStream);
impl_read_write_volatile_for_raw_fd!(std::os::unix::net::UnixStream);
impl_read_write_volatile_for_raw_fd!(std::os::fd::OwnedFd);
impl_read_write_volatile_for_raw_fd!(std::os::fd::BorrowedFd<'_>);

/// Tries to do a single `read` syscall on the provided file descriptor, storing the data raed in
/// the given [`VolatileSlice`].
///
/// Returns the numbers of bytes read.
fn read_volatile_raw_fd<Fd: AsRawFd>(
    raw_fd: &mut Fd,
    buf: &mut VolatileSlice<impl BitmapSlice>,
) -> Result<usize, VolatileMemoryError> {
    let fd = raw_fd.as_raw_fd();
    let guard = buf.ptr_guard_mut();

    let dst = guard.as_ptr().cast::<libc::c_void>();

    // SAFETY: We got a valid file descriptor from `AsRawFd`. The memory pointed to by `dst` is
    // valid for writes of length `buf.len() by the invariants upheld by the constructor
    // of `VolatileSlice`.
    let bytes_read = unsafe { libc::read(fd, dst, buf.len()) };

    if bytes_read < 0 {
        // We don't know if a partial read might have happened, so mark everything as dirty
        buf.bitmap().mark_dirty(0, buf.len());

        Err(VolatileMemoryError::IOError(std::io::Error::last_os_error()))
    } else {
        let bytes_read = bytes_read.try_into().unwrap();
        buf.bitmap().mark_dirty(0, bytes_read);
        Ok(bytes_read)
    }
}

/// Tries to do a single `write` syscall on the provided file descriptor, attempting to write the
/// data stored in the given [`VolatileSlice`].
///
/// Returns the numbers of bytes written.
fn write_volatile_raw_fd<Fd: AsRawFd>(
    raw_fd: &mut Fd,
    buf: &VolatileSlice<impl BitmapSlice>,
) -> Result<usize, VolatileMemoryError> {
    let fd = raw_fd.as_raw_fd();
    let guard = buf.ptr_guard();

    let src = guard.as_ptr().cast::<libc::c_void>();

    // SAFETY: We got a valid file descriptor from `AsRawFd`. The memory pointed to by `src` is
    // valid for reads of length `buf.len() by the invariants upheld by the constructor
    // of `VolatileSlice`.
    let bytes_written = unsafe { libc::write(fd, src, buf.len()) };

    if bytes_written < 0 {
        Err(VolatileMemoryError::IOError(std::io::Error::last_os_error()))
    } else {
        Ok(bytes_written.try_into().unwrap())
    }
}

impl WriteVolatile for &mut [u8] {
    fn write_volatile<B: BitmapSlice>(
        &mut self,
        buf: &VolatileSlice<B>,
    ) -> Result<usize, VolatileMemoryError> {
        let total = buf.len().min(self.len());
        let src = buf.subslice(0, total)?;

        // SAFETY:
        // We check above that `src` is contiguously allocated memory of length `total <= self.len())`.
        // Furthermore, both src and dst of the call to
        // copy_from_volatile_slice are valid for reads and writes respectively of length `total`
        // since total is the minimum of lengths of the memory areas pointed to. The areas do not
        // overlap, since `dst` is inside guest memory, and buf is a slice (no slices to guest
        // memory are possible without violating rust's aliasing rules).
        let written = unsafe { copy_from_volatile_slice(self.as_mut_ptr(), &src, total) };

        // Advance the slice, just like the stdlib: https://doc.rust-lang.org/src/std/io/impls.rs.html#335
        *self = std::mem::take(self).split_at_mut(written).1;

        Ok(written)
    }

    fn write_all_volatile<B: BitmapSlice>(
        &mut self,
        buf: &VolatileSlice<B>,
    ) -> Result<(), VolatileMemoryError> {
        // Based on https://github.com/rust-lang/rust/blob/f7b831ac8a897273f78b9f47165cf8e54066ce4b/library/std/src/io/impls.rs#L376-L382
        if self.write_volatile(buf)? == buf.len() {
            Ok(())
        } else {
            Err(VolatileMemoryError::IOError(std::io::Error::new(
                ErrorKind::WriteZero,
                "failed to write whole buffer",
            )))
        }
    }
}

impl ReadVolatile for &[u8] {
    fn read_volatile<B: BitmapSlice>(
        &mut self,
        buf: &mut VolatileSlice<B>,
    ) -> Result<usize, VolatileMemoryError> {
        let total = buf.len().min(self.len());
        let dst = buf.subslice(0, total)?;

        // SAFETY:
        // We check above that `dst` is contiguously allocated memory of length `total <= self.len())`.
        // Furthermore, both src and dst of the call to copy_to_volatile_slice are valid for reads
        // and writes respectively of length `total` since total is the minimum of lengths of the
        // memory areas pointed to. The areas do not overlap, since `dst` is inside guest memory,
        // and buf is a slice (no slices to guest memory are possible without violating rust's aliasing rules).
        let read = unsafe { copy_to_volatile_slice(&dst, self.as_ptr(), total) };

        // Advance the slice, just like the stdlib: https://doc.rust-lang.org/src/std/io/impls.rs.html#232-310
        *self = self.split_at(read).1;

        Ok(read)
    }

    fn read_exact_volatile<B: BitmapSlice>(
        &mut self,
        buf: &mut VolatileSlice<B>,
    ) -> Result<(), VolatileMemoryError> {
        // Based on https://github.com/rust-lang/rust/blob/f7b831ac8a897273f78b9f47165cf8e54066ce4b/library/std/src/io/impls.rs#L282-L302
        if buf.len() > self.len() {
            return Err(VolatileMemoryError::IOError(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            )));
        }

        self.read_volatile(buf).map(|_| ())
    }
}

// WriteVolatile implementation for Vec<u8> is based upon the Write impl for Vec, which
// defers to Vec::append_elements, after which the below functionality is modelled.
impl WriteVolatile for Vec<u8> {
    fn write_volatile<B: BitmapSlice>(
        &mut self,
        buf: &VolatileSlice<B>,
    ) -> Result<usize, VolatileMemoryError> {
        let count = buf.len();
        self.reserve(count);
        let len = self.len();

        // SAFETY: Calling Vec::reserve() above guarantees the the backing storage of the Vec has
        // length at least `len + count`. This means that self.as_mut_ptr().add(len) remains within
        // the same allocated object, the offset does not exceed isize (as otherwise reserve would
        // have panicked), and does not rely on address space wrapping around.
        // In particular, the entire `count` bytes after `self.as_mut_ptr().add(count)` is
        // contiguously allocated and valid for writes.
        // Lastly, `copy_to_volatile_slice` correctly initialized `copied_len` additional bytes
        // in the Vec's backing storage, and we assert this to be equal to `count`. Additionally,
        // `len + count` is at most the reserved capacity of the vector. Thus the call to `set_len`
        // is safe.
        unsafe {
            let copied_len = copy_from_volatile_slice(self.as_mut_ptr().add(len), buf, count);

            assert_eq!(copied_len, count);
            self.set_len(len + count);
        }
        Ok(count)
    }
}

// ReadVolatile and WriteVolatile implementations for Cursor<T> is modelled after the standard
// library's implementation (modulo having to inline `Cursor::remaining_slice`, as that's nightly only)
impl<T> ReadVolatile for Cursor<T>
where
    T: AsRef<[u8]>,
{
    fn read_volatile<B: BitmapSlice>(
        &mut self,
        buf: &mut VolatileSlice<B>,
    ) -> Result<usize, VolatileMemoryError> {
        let inner = self.get_ref().as_ref();
        let len = self.position().min(inner.len() as u64);
        let n = ReadVolatile::read_volatile(&mut &inner[(len as usize)..], buf)?;
        self.set_position(self.position() + n as u64);
        Ok(n)
    }

    fn read_exact_volatile<B: BitmapSlice>(
        &mut self,
        buf: &mut VolatileSlice<B>,
    ) -> Result<(), VolatileMemoryError> {
        let inner = self.get_ref().as_ref();
        let n = buf.len();
        let len = self.position().min(inner.len() as u64);
        ReadVolatile::read_exact_volatile(&mut &inner[(len as usize)..], buf)?;
        self.set_position(self.position() + n as u64);
        Ok(())
    }
}

impl WriteVolatile for Cursor<&mut [u8]> {
    fn write_volatile<B: BitmapSlice>(
        &mut self,
        buf: &VolatileSlice<B>,
    ) -> Result<usize, VolatileMemoryError> {
        let pos = self.position().min(self.get_ref().len() as u64);
        let n = WriteVolatile::write_volatile(&mut &mut self.get_mut()[(pos as usize)..], buf)?;
        self.set_position(self.position() + n as u64);
        Ok(n)
    }

    // no write_all provided in standard library, since our default for write_all is based on the
    // standard library's write_all, omitting it here as well will correctly mimic stdlib behavior.
}

#[cfg(test)]
mod tests {
    use crate::io::{ReadVolatile, WriteVolatile};
    use crate::{VolatileMemoryError, VolatileSlice};
    use std::io::{Cursor, ErrorKind, Read, Seek, Write};
    use vmm_sys_util::tempfile::TempFile;

    // ---- Test ReadVolatile for &[u8] ----
    fn read_4_bytes_to_5_byte_memory(source: Vec<u8>, expected_output: [u8; 5]) {
        // Test read_volatile for &[u8] works
        let mut memory = vec![0u8; 5];

        assert_eq!(
            (&source[..])
                .read_volatile(&mut VolatileSlice::from(&mut memory[..4]))
                .unwrap(),
            source.len().min(4)
        );
        assert_eq!(&memory, &expected_output);

        // Test read_exact_volatile for &[u8] works
        let mut memory = vec![0u8; 5];
        let result = (&source[..]).read_exact_volatile(&mut VolatileSlice::from(&mut memory[..4]));

        // read_exact fails if there are not enough bytes in input to completely fill
        // memory[..4]
        if source.len() < 4 {
            match result.unwrap_err() {
                VolatileMemoryError::IOError(ioe) => {
                    assert_eq!(ioe.kind(), ErrorKind::UnexpectedEof)
                }
                err => panic!("{:?}", err),
            }
            assert_eq!(memory, vec![0u8; 5]);
        } else {
            result.unwrap();
            assert_eq!(&memory, &expected_output);
        }
    }

    // ---- Test ReadVolatile for File ----
    fn read_4_bytes_from_file(source: Vec<u8>, expected_output: [u8; 5]) {
        let mut temp_file = TempFile::new().unwrap().into_file();
        temp_file.write_all(source.as_ref()).unwrap();
        temp_file.rewind().unwrap();

        // Test read_volatile for File works
        let mut memory = vec![0u8; 5];

        assert_eq!(
            temp_file
                .read_volatile(&mut VolatileSlice::from(&mut memory[..4]))
                .unwrap(),
            source.len().min(4)
        );
        assert_eq!(&memory, &expected_output);

        temp_file.rewind().unwrap();

        // Test read_exact_volatile for File works
        let mut memory = vec![0u8; 5];

        let read_exact_result =
            temp_file.read_exact_volatile(&mut VolatileSlice::from(&mut memory[..4]));

        if source.len() < 4 {
            read_exact_result.unwrap_err();
        } else {
            read_exact_result.unwrap();
        }
        assert_eq!(&memory, &expected_output);
    }

    #[test]
    fn test_read_volatile() {
        let test_cases = [
            (vec![1u8, 2], [1u8, 2, 0, 0, 0]),
            (vec![1, 2, 3, 4], [1, 2, 3, 4, 0]),
            // ensure we don't have a buffer overrun
            (vec![5, 6, 7, 8, 9], [5, 6, 7, 8, 0]),
        ];

        for (input, output) in test_cases {
            read_4_bytes_to_5_byte_memory(input.clone(), output);
            read_4_bytes_from_file(input, output);
        }
    }

    // ---- Test WriteVolatile for &mut [u8] ----
    fn write_4_bytes_to_5_byte_vec(mut source: Vec<u8>, expected_result: [u8; 5]) {
        let mut memory = vec![0u8; 5];

        // Test write_volatile for &mut [u8] works
        assert_eq!(
            (&mut memory[..4])
                .write_volatile(&VolatileSlice::from(source.as_mut_slice()))
                .unwrap(),
            source.len().min(4)
        );
        assert_eq!(&memory, &expected_result);

        // Test write_all_volatile for &mut [u8] works
        let mut memory = vec![0u8; 5];

        let result =
            (&mut memory[..4]).write_all_volatile(&VolatileSlice::from(source.as_mut_slice()));

        if source.len() > 4 {
            match result.unwrap_err() {
                VolatileMemoryError::IOError(ioe) => {
                    assert_eq!(ioe.kind(), ErrorKind::WriteZero)
                }
                err => panic!("{:?}", err),
            }
            // This quirky behavior of writing to the slice even in the case of failure is also
            // exhibited by the stdlib
            assert_eq!(&memory, &expected_result);
        } else {
            result.unwrap();
            assert_eq!(&memory, &expected_result);
        }
    }

    // ---- Test ẂriteVolatile for File works ----
    fn write_5_bytes_to_file(mut source: Vec<u8>) {
        // Test write_volatile for File works
        let mut temp_file = TempFile::new().unwrap().into_file();

        temp_file
            .write_volatile(&VolatileSlice::from(source.as_mut_slice()))
            .unwrap();
        temp_file.rewind().unwrap();

        let mut written = vec![0u8; source.len()];
        temp_file.read_exact(written.as_mut_slice()).unwrap();

        assert_eq!(source, written);
        // check no excess bytes were written to the file
        assert_eq!(temp_file.read(&mut [0u8]).unwrap(), 0);

        // Test write_all_volatile for File works
        let mut temp_file = TempFile::new().unwrap().into_file();

        temp_file
            .write_all_volatile(&VolatileSlice::from(source.as_mut_slice()))
            .unwrap();
        temp_file.rewind().unwrap();

        let mut written = vec![0u8; source.len()];
        temp_file.read_exact(written.as_mut_slice()).unwrap();

        assert_eq!(source, written);
        // check no excess bytes were written to the file
        assert_eq!(temp_file.read(&mut [0u8]).unwrap(), 0);
    }

    #[test]
    fn test_write_volatile() {
        let test_cases = [
            (vec![1u8, 2], [1u8, 2, 0, 0, 0]),
            (vec![1, 2, 3, 4], [1, 2, 3, 4, 0]),
            // ensure we don't have a buffer overrun
            (vec![5, 6, 7, 8, 9], [5, 6, 7, 8, 0]),
        ];

        for (input, output) in test_cases {
            write_4_bytes_to_5_byte_vec(input.clone(), output);
            write_5_bytes_to_file(input);
        }
    }

    #[test]
    fn test_read_volatile_for_cursor() {
        let read_buffer = [1, 2, 3, 4, 5, 6, 7];
        let mut output = vec![0u8; 5];

        let mut cursor = Cursor::new(read_buffer);

        // Read 4 bytes from cursor to volatile slice (amount read limited by volatile slice length)
        assert_eq!(
            cursor
                .read_volatile(&mut VolatileSlice::from(&mut output[..4]))
                .unwrap(),
            4
        );
        assert_eq!(output, vec![1, 2, 3, 4, 0]);

        // Read next 3 bytes from cursor to volatile slice (amount read limited by length of remaining data in cursor)
        assert_eq!(
            cursor
                .read_volatile(&mut VolatileSlice::from(&mut output[..4]))
                .unwrap(),
            3
        );
        assert_eq!(output, vec![5, 6, 7, 4, 0]);

        cursor.set_position(0);
        // Same as first test above, but with read_exact
        cursor
            .read_exact_volatile(&mut VolatileSlice::from(&mut output[..4]))
            .unwrap();
        assert_eq!(output, vec![1, 2, 3, 4, 0]);

        // Same as above, but with read_exact. Should fail now, because we cannot fill a 4 byte buffer
        // with whats remaining in the cursor (3 bytes). Output should remain unchanged.
        assert!(cursor
            .read_exact_volatile(&mut VolatileSlice::from(&mut output[..4]))
            .is_err());
        assert_eq!(output, vec![1, 2, 3, 4, 0]);
    }

    #[test]
    fn test_write_volatile_for_cursor() {
        let mut write_buffer = vec![0u8; 7];
        let mut input = [1, 2, 3, 4];

        let mut cursor = Cursor::new(write_buffer.as_mut_slice());

        // Write 4 bytes from volatile slice to cursor (amount written limited by volatile slice length)
        assert_eq!(
            cursor
                .write_volatile(&VolatileSlice::from(input.as_mut_slice()))
                .unwrap(),
            4
        );
        assert_eq!(cursor.get_ref(), &[1, 2, 3, 4, 0, 0, 0]);

        // Write 3 bytes from volatile slice to cursor (amount written limited by remaining space in cursor)
        assert_eq!(
            cursor
                .write_volatile(&VolatileSlice::from(input.as_mut_slice()))
                .unwrap(),
            3
        );
        assert_eq!(cursor.get_ref(), &[1, 2, 3, 4, 1, 2, 3]);
    }

    #[test]
    fn test_write_volatile_for_vec() {
        let mut write_buffer = Vec::new();
        let mut input = [1, 2, 3, 4];

        assert_eq!(
            write_buffer
                .write_volatile(&VolatileSlice::from(input.as_mut_slice()))
                .unwrap(),
            4
        );

        assert_eq!(&write_buffer, &input);
    }
}
