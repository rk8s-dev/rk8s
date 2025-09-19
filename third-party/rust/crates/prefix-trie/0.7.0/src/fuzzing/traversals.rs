use std::{
    sync::{Arc, Mutex},
    thread,
};

use itertools::Itertools;

use super::*;

qc!(children, _children);
fn _children((map, start): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let want = select_ref(&map, |p, _| start.contains(p));
    map.children(&start).eq(want)
}

qc!(children_trieview, _children_trieview);
fn _children_trieview((map, start): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let want = select_ref(&map, |p, _| start.contains(p));
    if let Some(view) = map.view_at(start) {
        view.iter().eq(want)
    } else {
        want.is_empty()
    }
}

qc!(children_trieview_mut, _children_trieview_mut);
fn _children_trieview_mut((mut map, start): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let want = select(&map, |p, _| start.contains(p));
    if let Some(view) = map.view_mut_at(start) {
        view.into_iter().map(|(p, t)| (*p, *t)).eq(want)
    } else {
        want.is_empty()
    }
}

qc!(children_keys_trieview, _children_keys_trieview);
fn _children_keys_trieview((map, start): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let want = select_keys(&map, |p, _| start.contains(p));
    if let Some(view) = map.view_at(start) {
        view.keys().eq(want)
    } else {
        want.is_empty()
    }
}

qc!(children_values_trieview, _children_values_trieview);
fn _children_values_trieview((map, start): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let want = select_values(&map, |p, _| start.contains(p));
    if let Some(view) = map.view_at(start) {
        view.values().eq(want)
    } else {
        want.is_empty()
    }
}

qc!(simultaneous_iter_mut, _simultaneous_iter_mut);
fn _simultaneous_iter_mut(mut map: PrefixMap<TestPrefix, i32>) -> bool {
    // go to the first split
    let mut view = map.view_mut();
    let (left, right) = loop {
        match view.split() {
            (None, None) => return true,
            (None, Some(v)) | (Some(v), None) => view = v,
            (Some(left), Some(right)) => break (left, right),
        }
    };

    let want = left
        .view()
        .into_iter()
        .chain(right.view())
        .map(|(p, t)| (*p, t.saturating_mul(4)))
        .sorted()
        .collect::<Vec<_>>();

    // simultaneously traverse left and right
    let result = Arc::new(Mutex::new(Vec::new()));
    let res_left = result.clone();
    let res_right = result.clone();

    fn foo<'a>(
        view: TrieViewMut<'a, TestPrefix, i32>,
        res: Arc<Mutex<Vec<(&'a TestPrefix, &'a mut i32)>>>,
    ) {
        let mut references = Vec::new();
        for (p, t) in view {
            *t = t.saturating_mul(2);
            references.push((p, t));
        }
        res.lock().unwrap().extend(references);
    }

    thread::scope(move |s| {
        let thread_left = s.spawn(move || foo(left, res_left));
        let thread_right = s.spawn(move || foo(right, res_right));
        thread_left.join().unwrap();
        thread_right.join().unwrap();
    });

    // take the result and multiply all values by 2 again
    let mut result = result.lock().unwrap();
    let mut got = Vec::new();
    for (p, t) in result.iter_mut() {
        **t = t.saturating_mul(2);
        got.push((**p, **t));
    }
    got.sort();

    want == got
}
