#![allow(clippy::no_effect_underscore_binding)]

use ghost::phantom;

pub trait Trait<T> {}

#[phantom]
pub struct Independent<W: ?Sized, const I: u32>;

#[phantom]
pub struct Dependent<const I: usize, T: Trait<[u8; I]>>;

#[test]
fn test_const() {
    let _phantom_v = Independent::<i8, 0>;
    let _phantom_t: Independent<i8, 0> = Independent;
}
