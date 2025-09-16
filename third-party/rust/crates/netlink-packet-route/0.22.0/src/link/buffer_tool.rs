// SPDX-License-Identifier: MIT

pub(crate) fn expand_buffer_if_small(
    got: &[u8],
    expected_size: usize,
    nla_name: &str,
) -> Vec<u8> {
    let mut payload = got.to_vec();
    match payload.len() {
        l if l > expected_size => {
            log::warn!(
                "Specified {nla_name} NLA attribute holds \
            more(most likely new kernel) data which is unknown to \
            netlink-packet-route crate, expecting \
            {expected_size}, got {}",
                got.len()
            );
        }
        l if l < expected_size => {
            payload.extend_from_slice(&vec![0; expected_size - got.len()]);
        }
        _ => (),
    }
    payload
}
