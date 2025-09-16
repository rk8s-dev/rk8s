pub trait VecExt {
    // slice has a method named `strip_prefix`
    fn strip_key_prefix(&mut self, prefix: &[u8]);
    fn prefix_with(&mut self, prefix: &[u8]);
    fn prefix_range_end_with(&mut self, prefix: &[u8]);
}

impl VecExt for Vec<u8> {
    fn strip_key_prefix(&mut self, prefix: &[u8]) {
        if prefix.is_empty() {
            return;
        }

        if !self.starts_with(prefix) {
            return;
        }

        let pfx_len = prefix.len();
        let key_len = self.len();
        let new_len = key_len - pfx_len;
        unsafe {
            let ptr = self.as_mut_ptr();
            std::ptr::copy(ptr.add(pfx_len), ptr, new_len);
            self.set_len(new_len);
        }
    }

    fn prefix_with(&mut self, prefix: &[u8]) {
        if prefix.is_empty() {
            return;
        }

        let pfx_len = prefix.len();
        let key_len = self.len();
        self.reserve(pfx_len);
        unsafe {
            let ptr = self.as_mut_ptr();
            std::ptr::copy(ptr, ptr.add(pfx_len), key_len);
            std::ptr::copy_nonoverlapping(prefix.as_ptr(), ptr, pfx_len);
            self.set_len(key_len + pfx_len);
        }
    }

    fn prefix_range_end_with(&mut self, prefix: &[u8]) {
        if prefix.is_empty() {
            return;
        }

        if self.len() == 1 && self[0] == 0 {
            // the edge of the keyspace
            self.clear();
            self.extend_from_slice(prefix);
            let mut ok = false;
            for i in (0..self.len()).rev() {
                self[i] = self[i].wrapping_add(1);
                if self[i] != 0 {
                    ok = true;
                    break;
                }
            }
            if !ok {
                // 0xff..ff => 0x00
                self.clear();
                self.push(0);
            }
        } else if !self.is_empty() {
            self.prefix_with(prefix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_with() {
        fn assert_prefix_with(pfx: &[u8], key: &[u8], end: &[u8], w_key: &[u8], w_end: &[u8]) {
            let mut key = key.to_vec();
            let mut end = end.to_vec();
            key.prefix_with(pfx);
            end.prefix_range_end_with(pfx);
            assert_eq!(key, w_key);
            assert_eq!(end, w_end);
        }

        // single key
        assert_prefix_with(b"pfx/", b"a", b"", b"pfx/a", b"");

        // range
        assert_prefix_with(b"pfx/", b"abc", b"def", b"pfx/abc", b"pfx/def");

        // one-sided range (HACK - b'/' + 1 = b'0')
        assert_prefix_with(b"pfx/", b"abc", b"\0", b"pfx/abc", b"pfx0");

        // one-sided range, end of keyspace
        assert_prefix_with(b"\xFF\xFF", b"abc", b"\0", b"\xff\xffabc", b"\0");
    }
}
