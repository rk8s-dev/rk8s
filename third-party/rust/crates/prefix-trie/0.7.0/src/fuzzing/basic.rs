use std::collections::HashMap;

use super::*;
use itertools::Itertools;

qc!(new, _new);
fn _new(list: Vec<(TestPrefix, i32)>) -> bool {
    let mut pmap = PrefixMap::new();
    let mut hmap = HashMap::new();

    for (p, t) in list {
        pmap.insert(p, t);
        hmap.insert(p, t);
    }

    // assert that the iterator of both is the same
    pmap.into_iter().eq(hmap.into_iter().sorted())
}

qc!(new_mods, _new_mods);
fn _new_mods(list: Vec<Operation<TestPrefix, i32>>) -> bool {
    let mut pmap = PrefixMap::new();
    let mut hmap = HashMap::new();

    for op in list {
        match op {
            Operation::Add(p, t) => {
                pmap.insert(p, t);
                hmap.insert(p, t);
            }
            Operation::Remove(p) => {
                pmap.remove(&p);
                hmap.remove(&p);
            }
        }
    }

    // assert that the iterator of both is the same
    pmap.into_iter().eq(hmap.into_iter().sorted())
}

qc!(new_mods_entry, _new_mods_entry);
fn _new_mods_entry(list: Vec<Operation<TestPrefix, i32>>) -> bool {
    let mut pmap = PrefixMap::new();
    let mut hmap = HashMap::new();

    for op in list {
        match op {
            Operation::Add(p, t) => {
                let _ = pmap.entry(p).insert(t);
                hmap.insert(p, t);
            }
            Operation::Remove(p) => {
                pmap.remove(&p);
                hmap.remove(&p);
            }
        }
    }

    // assert that the iterator of both is the same
    pmap.into_iter().eq(hmap.into_iter().sorted())
}

qc!(equality, _equality);
fn _equality(list: Vec<Operation<TestPrefix, i32>>) -> bool {
    let mut map = PrefixMap::default();

    for op in list {
        match op {
            Operation::Add(p, t) => {
                map.insert(p, t);
            }
            Operation::Remove(p) => {
                map.remove(&p);
            }
        }
    }

    let clone = map.clone().into_iter().collect::<PrefixMap<_, _>>();

    // assert that the iterator of both is the same
    map == clone && map.len() == clone.len() && map.is_empty() == clone.is_empty()
}

qc!(equality_keep_tree, _equality_keep_tree);
fn _equality_keep_tree(list: Vec<Operation<TestPrefix, i32>>) -> bool {
    let mut map = PrefixMap::default();

    for op in list {
        match op {
            Operation::Add(p, t) => {
                map.insert(p, t);
            }
            Operation::Remove(p) => {
                map.remove_keep_tree(&p);
            }
        }
    }

    let clone = map.clone().into_iter().collect::<PrefixMap<_, _>>();

    // assert that the iterator of both is the same
    map == clone && map.len() == clone.len() && map.is_empty() == clone.is_empty()
}

qc!(equality_set, _equality_set);
fn _equality_set(list: Vec<Operation<TestPrefix, ()>>) -> bool {
    let mut set = PrefixSet::default();

    for op in list {
        match op {
            Operation::Add(p, _) => {
                set.insert(p);
            }
            Operation::Remove(p) => {
                set.remove(&p);
            }
        }
    }

    let clone = set.iter().copied().collect::<PrefixSet<_>>();

    // assert that the iterator of both is the same
    set == clone && set.len() == clone.len() && set.is_empty() == clone.is_empty()
}

qc!(remove_children, _remove_children);
fn _remove_children((mut map, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let want = select(&map, |p, _| !root.contains(p));
    map.remove_children(&root);
    map.len() == want.len() && map.into_iter().eq(want)
}

qc!(retain, _retain);
fn _retain((mut map, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let want = select(&map, |p, _| !(root.contains(p) && p.1 >= root.1 + 2));
    map.retain(|p, _| !(root.contains(p) && p.1 >= root.1 + 2));
    map.into_iter().eq(want)
}

qc!(view_at, _view_at);
fn _view_at((map, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let value = map.get(&root).cloned();
    let root_has_nodes = map.iter().any(|(p, _)| root.contains(p));
    match map.view_at(root) {
        None => !root_has_nodes,
        Some(view) => view.value() == value.as_ref(),
    }
}
qc!(view_mut_at, _view_mut_at);
fn _view_mut_at((mut map, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let value = map.get(&root).cloned();
    let root_has_nodes = map.iter().any(|(p, _)| root.contains(p));
    match map.view_mut_at(root) {
        None => !root_has_nodes,
        Some(view) => view.value() == value.as_ref(),
    }
}

qc!(view_left, _view_left);
fn _view_left((map, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let left_prefix_has_nodes = map.iter().any(|(p, _)| root.left().contains(p));
    map.view_at(root).and_then(|v| v.left()).is_some() == left_prefix_has_nodes
}

qc!(view_right, _view_right);
fn _view_right((map, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let right_prefix_has_nodes = map.iter().any(|(p, _)| root.right().contains(p));
    map.view_at(root).and_then(|v| v.right()).is_some() == right_prefix_has_nodes
}

qc!(view_mut_left, _view_mut_left);
fn _view_mut_left((mut m, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let left_prefix_has_nodes = m.iter().any(|(p, _)| root.left().contains(p));
    let c1 = m.view_mut_at(root).and_then(|v| v.left().ok()).is_some() == left_prefix_has_nodes;
    let c2 = m.view_mut_at(root).map(|v| v.has_left()).unwrap_or(false) == left_prefix_has_nodes;
    c1 && c2
}

qc!(view_mut_right, _view_mut_right);
fn _view_mut_right((mut m, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let right_prefix_has_nodes = m.iter().any(|(p, _)| root.right().contains(p));
    let c1 = m.view_mut_at(root).and_then(|v| v.right().ok()).is_some() == right_prefix_has_nodes;
    let c2 = m.view_mut_at(root).map(|v| v.has_right()).unwrap_or(false) == right_prefix_has_nodes;
    c1 && c2
}

qc!(view_mut_split, _view_mut_split);
fn _view_mut_split((mut map, root): (PrefixMap<TestPrefix, i32>, TestPrefix)) -> bool {
    let left_prefix_has_nodes = map.iter().any(|(p, _)| root.left().contains(p));
    let right_prefix_has_nodes = map.iter().any(|(p, _)| root.right().contains(p));
    map.view_mut_at(root)
        .map(|view| view.split())
        .map(|(l, r)| (l.is_some(), r.is_some()))
        .unwrap_or((false, false))
        == (left_prefix_has_nodes, right_prefix_has_nodes)
}
