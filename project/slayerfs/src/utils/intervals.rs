use std::cmp::{max, min};

pub struct Intervals<T: Copy + Ord>(Vec<(T, T)>);

impl<T: Copy + Ord> Intervals<T> {
    pub fn new(l: T, r: T) -> Self {
        debug_assert!(l <= r, "invalid interval: left must be <= right");
        Intervals(vec![(l, r)])
    }

    pub fn cut(&mut self, slice_l: T, slice_r: T) -> Vec<(T, T)> {
        if self.0.is_empty() {
            return Vec::new();
        }

        let mut remaining = Vec::new();
        let mut cut = Vec::new();
        let mut touched = false;

        for &(l, r) in &self.0 {
            if r <= slice_l || l >= slice_r {
                remaining.push((l, r));
                continue;
            }

            touched = true;
            let cut_l = max(l, slice_l);
            let cut_r = min(r, slice_r);
            if cut_l < cut_r {
                cut.push((cut_l, cut_r));
            }
            if l < cut_l {
                remaining.push((l, cut_l));
            }
            if cut_r < r {
                remaining.push((cut_r, r));
            }
        }

        if !touched {
            return Vec::new();
        }

        self.0 = remaining.into_iter().filter(|(l, r)| l < r).collect();
        cut
    }

    pub fn collect(self) -> Vec<(T, T)> {
        self.0
    }
}
