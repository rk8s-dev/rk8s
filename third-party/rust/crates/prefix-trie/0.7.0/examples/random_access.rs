use std::net::Ipv4Addr;

use prefix_trie::*;

use ipnet::Ipv4Net;
use rand::prelude::*;

fn main() {
    let mut pm = PrefixMap::<Ipv4Net, u32>::new();

    let mut rng = thread_rng();

    for _ in 0..1_000_000 {
        let prefix = Ipv4Net::new(Ipv4Addr::new(rng.gen(), 0, 0, 0), rng.gen_range(1..=8)).unwrap();
        let prefix = Ipv4Net::new(prefix.mask().into(), prefix.prefix_len()).unwrap();

        if rng.gen_bool(0.7) {
            let value: u32 = rng.gen::<u8>() as u32;
            pm.insert(prefix, value);
        } else if rng.gen_bool(0.1) {
            // remove all children of that prefix
            pm.remove_children(&prefix);
        } else {
            pm.remove(&prefix);
        }
    }
}
