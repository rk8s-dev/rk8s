//! Module for testing using fuzzing (quickcheck)
#![allow(clippy::type_complexity)]

use std::fmt::Debug;

use crate::*;
use quickcheck::Arbitrary;

mod basic;
mod set_ops;
mod traversals;

#[derive(Debug, PartialEq, Clone, Copy)]
enum Operation<P, T> {
    Add(P, T),
    Remove(P),
}

#[cfg(miri)]
const DEFAULT_NUM_TESTS: usize = 10;
#[cfg(not(miri))]
const DEFAULT_NUM_TESTS: usize = 10000;
const DEFAULT_GEN_SIZE: usize = 100;

fn proptest_runner<A: Arbitrary + Debug + PartialEq, F: Fn(A) -> bool>(f: F) {
    let num_tests: usize = std::env::var("QUICKCHECK_TESTS")
        .ok()
        .and_then(|x| x.parse::<usize>().ok())
        .unwrap_or(DEFAULT_NUM_TESTS);

    let gen_size: usize = std::env::var("QUICKCHECK_GENERATOR_SIZE")
        .ok()
        .and_then(|x| x.parse::<usize>().ok())
        .unwrap_or(DEFAULT_GEN_SIZE);

    let mut gen = quickcheck::Gen::new(gen_size);

    // sample all inputs
    for _ in 0..num_tests {
        let input = A::arbitrary(&mut gen);
        let input_c = input.clone();
        let success = f(input_c);
        if !success {
            shrink_failure(f, input)
        }
    }
}

fn shrink_failure<A: Arbitrary + Debug + PartialEq, F: Fn(A) -> bool>(f: F, input: A) -> ! {
    for i in input.shrink() {
        let i_c = i.clone();
        let success = f(i_c);
        if !success {
            shrink_failure(f, i)
        }
    }
    // if we reach this point, then all shrunken inputs work. Therefore, `inputs` is the minimal
    // input
    panic!(
        "[QUICKCHECK] Test case failed!\n  Minimal input:\n    {:?}",
        input
    );
}

#[allow(missing_docs)]
#[macro_export]
macro_rules! qc {
    ($name:ident, $f:ident) => {
        #[test]
        fn $name() {
            proptest_runner($f)
        }
    };
}

fn select<P: Clone, T: Clone, F: Fn(&P, &T) -> bool>(map: &PrefixMap<P, T>, f: F) -> Vec<(P, T)> {
    map.iter()
        .map(|(p, t)| (p.clone(), t.clone()))
        .filter(|(p, t)| f(p, t))
        .collect()
}

fn select_ref<P, T, F: Fn(&P, &T) -> bool>(map: &PrefixMap<P, T>, f: F) -> Vec<(&P, &T)> {
    map.iter().filter(|(p, t)| f(*p, *t)).collect()
}

fn select_keys<P, T, F: Fn(&P, &T) -> bool>(map: &PrefixMap<P, T>, f: F) -> Vec<&P> {
    map.iter()
        .filter(|(p, t)| f(*p, *t))
        .map(|(p, _)| p)
        .collect()
}

fn select_values<P, T, F: Fn(&P, &T) -> bool>(map: &PrefixMap<P, T>, f: F) -> Vec<&T> {
    map.iter()
        .filter(|(p, t)| f(*p, *t))
        .map(|(_, t)| t)
        .collect()
}

impl<P: Prefix + Arbitrary, T: Arbitrary> Arbitrary for PrefixMap<P, T> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        <Vec<(P, T)> as Arbitrary>::arbitrary(g)
            .into_iter()
            .collect()
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let elems = self.clone().into_iter().collect::<Vec<_>>();
        let shrinked = elems.shrink();
        Box::new(shrinked.map(PrefixMap::from_iter))
    }
}

impl<P: Arbitrary, T: Arbitrary> Arbitrary for Operation<P, T> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let p = P::arbitrary(g);
        if g.choose(&[
            true, true, true, true, true, true, true, false, false, false,
        ])
        .copied()
        .unwrap_or_default()
        {
            let t = T::arbitrary(g);
            Self::Add(p, t)
        } else {
            Self::Remove(p)
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            Operation::Add(p, t) => {
                let t = t.clone();
                Box::new(
                    p.clone()
                        .shrink()
                        .map(move |p| Operation::Add(p, t.clone())),
                )
            }
            Operation::Remove(p) => Box::new(p.clone().shrink().map(|p| Operation::Remove(p))),
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct TestPrefix(u32, u8);

impl TestPrefix {
    fn left(self) -> Self {
        TestPrefix(self.0, self.1 + 1)
    }

    fn right(self) -> Self {
        TestPrefix(self.0 + (1 << (31 - self.1)), self.1 + 1)
    }
}

impl Debug for TestPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let addr = format!("{:032b}", self.0)[..10].to_string();
        write!(f, "0b{addr}/{}", self.1)
    }
}

impl Arbitrary for TestPrefix {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        #[rustfmt::skip]
        let len: u8 = *g
            .choose(&[
                0,
                1, 1,
                2, 2, 2,
                3, 3, 3, 3,
                4, 4, 4, 4, 4,
                5, 5, 5, 5, 5, 5,
                6, 6, 6, 6, 6, 6, 6,
                7, 7, 7, 7, 7, 7, 7, 7,
                8, 8, 8, 8, 8, 8, 8, 8, 8,
                9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
            ])
            .unwrap();
        let x = u32::arbitrary(g);
        Self::from_repr_len(x, len)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        if self.1 == 0 {
            quickcheck::empty_shrinker()
        } else {
            let len = self.1 - 1;
            let x = Self::from_repr_len(self.0, len);
            quickcheck::single_shrinker(x)
        }
    }
}

impl Prefix for TestPrefix {
    type R = u32;

    fn repr(&self) -> Self::R {
        self.0
    }

    fn prefix_len(&self) -> u8 {
        self.1
    }

    fn from_repr_len(repr: Self::R, len: u8) -> Self {
        let x = Prefix::mask(&(repr, len));
        Self(x, len)
    }
}
