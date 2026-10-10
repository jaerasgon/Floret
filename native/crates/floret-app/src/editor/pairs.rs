//! Write → Read feedback pairs (`state/write` → `state/read`, and the
//! spread versions). The only way a value crosses frames: the Write sits
//! at the end of the flow, its Read at the start. The two halves are
//! matched by their `slot` (kept underneath); a pair can carry a `name`,
//! shared by both halves.
//!
//! Editing keeps pairs whole: placing one half places the other, copies
//! get a fresh slot (and a missing half), deleting a half takes its
//! partner, renaming one renames both.

use super::ports::find_node;
use floret_engine::patch::{PatchFile, PatchNode, Position};
use floret_engine::spec::Catalogue;
use serde_json::Value as Json;
use std::collections::HashSet;

/// Horizontal step between a new half and the flow it is placed beside
/// (a node's width plus room for wires, a multiple of the grid steps).
const STEP: f64 = 240.0;
/// Vertical step when that spot is taken.
const DROP: f64 = 96.0;
/// Names longer than this are cut short in the header.
const TITLE_CHARS: usize = 28;

/// A node type's family (its Read and Write types) and whether it is the
/// Write half.
fn family(def_type: &str) -> Option<((&'static str, &'static str), bool)> {
    floret_engine::compile::FEEDBACK_FAMILIES
        .into_iter()
        .find_map(|(r, w)| {
            if def_type == r {
                Some(((r, w), false))
            } else if def_type == w {
                Some(((r, w), true))
            } else {
                None
            }
        })
}

/// Whether `def_type` is a Read or Write half.
pub fn is_pair_type(def_type: &str) -> bool {
    family(def_type).is_some()
}

/// Whether `def_type` is the Write half.
pub fn is_write(def_type: &str) -> bool {
    family(def_type).is_some_and(|(_, w)| w)
}

/// The slot as the engine keys it (`1` and `1.0` are one slot).
fn slot_key(n: &PatchNode) -> Option<String> {
    match n.params.get("slot")? {
        Json::String(s) => Some(s.clone()),
        Json::Number(v) => v.as_f64().map(|v| v.to_string()),
        _ => None,
    }
}

/// `(read type, slot)`: the pair a half belongs to.
fn pair_key(n: &PatchNode) -> Option<(&'static str, String)> {
    let ((r, _), _) = family(&n.def_type)?;
    Some((r, slot_key(n)?))
}

/// The pair's name (trimmed), if it has one.
pub fn name(n: &PatchNode) -> Option<&str> {
    floret_engine::compile::pair_name(&n.params)
}

/// The header title of a named half: `Write · ball position`.
pub fn title(n: &PatchNode, label: &str) -> Option<String> {
    let name = name(n).filter(|_| is_pair_type(&n.def_type))?;
    let short: String = if name.chars().count() > TITLE_CHARS {
        let cut: String = name.chars().take(TITLE_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        name.to_owned()
    };
    Some(format!("{label} · {short}"))
}

/// Every half of the pair `id` belongs to (`id` included), in patch order.
pub fn group(patch: &PatchFile, id: &str) -> Vec<String> {
    let Some(key) = find_node(patch, id).and_then(pair_key) else {
        return Vec::new();
    };
    patch
        .nodes
        .iter()
        .filter(|n| pair_key(n).as_ref() == Some(&key))
        .map(|n| n.id.clone())
        .collect()
}

/// The other half (or halves) of `id`'s pair: its Read(s) for a Write,
/// its Write for a Read.
pub fn partners(patch: &PatchFile, id: &str) -> Vec<String> {
    let Some(write) = find_node(patch, id).map(|n| is_write(&n.def_type)) else {
        return Vec::new();
    };
    group(patch, id)
        .into_iter()
        .filter(|g| find_node(patch, g).is_some_and(|n| is_write(&n.def_type) != write))
        .collect()
}

/// The smallest slot (1, 2, …) no Read or Write of `def_type`'s family
/// uses, ignoring the nodes in `skip`.
pub fn fresh_slot(patch: &PatchFile, def_type: &str, skip: &HashSet<&str>) -> i64 {
    let Some(((r, _), _)) = family(def_type) else {
        return 1;
    };
    let used: HashSet<String> = patch
        .nodes
        .iter()
        .filter(|n| !skip.contains(n.id.as_str()))
        .filter_map(pair_key)
        .filter(|(fr, _)| *fr == r)
        .map(|(_, s)| s)
        .collect();
    (1..).find(|s| !used.contains(&s.to_string())).unwrap_or(1)
}

/// Rename the pair `id` belongs to: every half gets `name` (blank clears
/// it). Returns whether anything changed.
pub fn rename(patch: &mut PatchFile, id: &str, name: &str) -> bool {
    let name = name.trim();
    let ids: HashSet<String> = group(patch, id).into_iter().collect();
    let mut changed = false;
    for n in patch.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
        let before = n.params.get("name").cloned();
        if name.is_empty() {
            n.params.remove("name");
        } else {
            n.params
                .insert("name".into(), Json::String(name.to_owned()));
        }
        changed |= n.params.get("name").cloned() != before;
    }
    changed
}

/// Where the missing half of `id` goes: a Read to the left of its scope's
/// flow, a Write to the right of it, at `id`'s height, below any node
/// already there. Local to `id`'s parent.
fn partner_position(patch: &PatchFile, cat: &Catalogue, id: &str) -> Position {
    let n = find_node(patch, id).expect("half exists");
    let siblings: Vec<&PatchNode> = patch
        .nodes
        .iter()
        .filter(|s| s.id != id && s.parent_id == n.parent_id)
        .collect();
    // The flow's extent: comments sit beside it, not in it.
    let xs = siblings
        .iter()
        .filter(|s| s.def_type != "meta/comment")
        .map(|s| s.position.x);
    let mut x = if is_write(&n.def_type) {
        xs.fold(n.position.x, f64::min) - STEP
    } else {
        xs.fold(n.position.x, f64::max) + STEP
    };
    // Inside a ForEach / If / Cache frame, stay within its left edge.
    let in_frame = n
        .parent_id
        .as_deref()
        .and_then(|p| find_node(patch, p))
        .and_then(|p| cat.get(&p.def_type))
        .is_some_and(|s| super::regions::is_drawable(s.region_kind));
    if in_frame {
        x = x.max(24.0);
    }
    let mut y = n.position.y;
    let taken = |y: f64| {
        siblings.iter().any(|s| {
            (s.position.x - x).abs() < STEP - 48.0 && (s.position.y - y).abs() < DROP - 16.0
        })
    };
    for _ in 0..32 {
        if !taken(y) {
            break;
        }
        y += DROP;
    }
    Position { x, y }
}

/// Add the missing other half of `id` (same family, slot, name and
/// parent), placed beside the flow. Returns its id.
pub fn add_partner(patch: &mut PatchFile, cat: &Catalogue, id: &str) -> Option<String> {
    let n = find_node(patch, id)?.clone();
    let ((r, w), write) = family(&n.def_type)?;
    let ty = if write { r } else { w };
    let pos = partner_position(patch, cat, id);
    let new = super::ops::fresh_node_id(patch, ty);
    let mut params = cat.get(ty)?.default_params.clone();
    for k in ["slot", "name"] {
        match n.params.get(k) {
            Some(v) => params.insert(k.into(), v.clone()),
            None => params.remove(k),
        };
    }
    patch.nodes.push(PatchNode {
        id: new.clone(),
        def_type: ty.to_owned(),
        position: pos,
        params,
        parent_id: n.parent_id.clone(),
        port_values: None,
        ports: None,
        width: None,
        height: None,
        bypassed: false,
    });
    Some(new)
}

/// After a duplicate or paste: give each copied pair a slot of its own,
/// so the copies don't share state with the originals, and add the half
/// a lone copied Read or Write is missing. Returns the added halves.
pub fn reslot_copies(patch: &mut PatchFile, cat: &Catalogue, copies: &[String]) -> Vec<String> {
    // Copied halves by the pair they were copied from, in copy order.
    let mut groups: Vec<((&'static str, String), Vec<String>)> = Vec::new();
    for id in copies {
        let Some(key) = find_node(patch, id).and_then(pair_key) else {
            continue;
        };
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, ids)) => ids.push(id.clone()),
            None => groups.push((key, vec![id.clone()])),
        }
    }
    let mut added = Vec::new();
    for ((read_ty, _), ids) in groups {
        let skip: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let slot = fresh_slot(patch, read_ty, &skip);
        for n in patch
            .nodes
            .iter_mut()
            .filter(|n| skip.contains(n.id.as_str()))
        {
            n.params.insert("slot".into(), slot.into());
        }
        let has = |write: bool| {
            ids.iter()
                .any(|i| find_node(patch, i).is_some_and(|n| is_write(&n.def_type) == write))
        };
        if !(has(true) && has(false)) {
            added.extend(add_partner(patch, cat, &ids[0]));
        }
    }
    added
}

/// `ids` plus the halves that go with them when deleted: a Write takes its
/// Reads; a Read takes its Write unless another of its Reads stays.
pub fn with_partners(patch: &PatchFile, ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ids.to_vec();
    let gone: HashSet<&str> = ids.iter().map(String::as_str).collect();
    for id in ids {
        let Some(n) = find_node(patch, id).filter(|n| is_pair_type(&n.def_type)) else {
            continue;
        };
        let write = is_write(&n.def_type);
        let group = group(patch, id);
        let reads_left = group.iter().any(|g| {
            !gone.contains(g.as_str())
                && find_node(patch, g).is_some_and(|m| !is_write(&m.def_type))
        });
        for g in partners(patch, id) {
            if (write || !reads_left) && !out.contains(&g) {
                out.push(g);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::ops::{delete_nodes, duplicate, place_node};
    use crate::editor::test_util::{node, patch};
    use serde_json::json;

    fn half(id: &str, ty: &str, slot: i64, x: f64) -> PatchNode {
        let mut n = node(id, ty);
        n.params.insert("slot".into(), slot.into());
        n.position = Position { x, y: 0.0 };
        n
    }

    fn slot(p: &PatchFile, id: &str) -> Json {
        find_node(p, id).unwrap().params["slot"].clone()
    }

    #[test]
    fn placing_a_write_places_its_read_left_of_the_flow() {
        let cat = Catalogue::builtin();
        let mut add = node("a", "math/add");
        add.position = Position { x: 100.0, y: 40.0 };
        // Slot 1 is taken by a Read; the new pair takes 2, the same for both.
        let mut p = patch(vec![add, half("r", "state/read", 1, 0.0)], vec![]);
        let w = place_node(
            &mut p,
            cat,
            "state/write",
            Position { x: 600.0, y: 200.0 },
            None,
        )
        .unwrap();
        let r = partners(&p, &w);
        assert_eq!(r.len(), 1);
        let rn = find_node(&p, &r[0]).unwrap();
        assert_eq!(rn.def_type, "state/read");
        assert_eq!(slot(&p, &w), 2);
        assert_eq!(slot(&p, &r[0]), 2);
        assert_eq!(
            rn.position,
            Position {
                x: -240.0,
                y: 200.0
            }
        );
        // And the other way round: a Read* places its Write* to the right.
        let rs = place_node(&mut p, cat, "state/read-spread", Position::default(), None).unwrap();
        let ws = partners(&p, &rs);
        let wn = find_node(&p, &ws[0]).unwrap();
        assert_eq!(wn.def_type, "state/write-spread");
        assert_eq!(slot(&p, &rs), 1, "spread pairs number on their own");
        assert!(wn.position.x > 600.0);
    }

    #[test]
    fn renaming_one_half_renames_its_partner_only() {
        let mut p = patch(
            vec![
                half("r", "state/read", 1, 0.0),
                half("w", "state/write", 1, 300.0),
                half("r2", "state/read", 2, 0.0),
                half("w2", "state/write", 2, 300.0),
                half("rs", "state/read-spread", 1, 0.0),
            ],
            vec![],
        );
        assert!(rename(&mut p, "w", " ball position "));
        assert_eq!(name(find_node(&p, "r").unwrap()), Some("ball position"));
        assert_eq!(name(find_node(&p, "w").unwrap()), Some("ball position"));
        for other in ["r2", "w2", "rs"] {
            assert_eq!(name(find_node(&p, other).unwrap()), None, "{other}");
        }
        assert!(!rename(&mut p, "r", "ball position"), "no change");
        let label = Catalogue::builtin()
            .get("state/write")
            .unwrap()
            .label
            .clone();
        assert_eq!(
            title(find_node(&p, "w").unwrap(), &label).as_deref(),
            Some("Write · ball position")
        );
        assert_eq!(title(find_node(&p, "w2").unwrap(), &label), None);
        // A blank name clears both, in one undo step.
        let mut history = crate::editor::history::History::default();
        history.reset(&p);
        assert!(rename(&mut p, "r", ""));
        history.push(&p, None);
        assert!(!find_node(&p, "w").unwrap().params.contains_key("name"));
        assert!(history.undo(&mut p));
        assert_eq!(name(find_node(&p, "r").unwrap()), Some("ball position"));
        assert_eq!(name(find_node(&p, "w").unwrap()), Some("ball position"));
    }

    #[test]
    fn deleting_a_half_deletes_its_partner() {
        let mut p = patch(
            vec![
                half("r", "state/read", 1, 0.0),
                half("w", "state/write", 1, 300.0),
                half("r2", "state/read", 2, 0.0),
                half("r2b", "state/read", 2, 0.0),
                half("w2", "state/write", 2, 300.0),
            ],
            vec![],
        );
        let ids = with_partners(&p, &["r".into()]);
        assert_eq!(ids, ["r", "w"]);
        // A second Read of the same Write goes alone; the Write takes both.
        assert_eq!(with_partners(&p, &["r2".into()]), ["r2"]);
        assert_eq!(with_partners(&p, &["w2".into()]), ["w2", "r2", "r2b"]);
        let gone = with_partners(&p, &["w".into()]);
        delete_nodes(&mut p, &gone);
        assert!(find_node(&p, "r").is_none() && find_node(&p, "w").is_none());
    }

    #[test]
    fn copies_get_a_fresh_slot_and_a_lone_write_gets_a_read() {
        let cat = Catalogue::builtin();
        let mut r = half("r", "state/read", 1, 0.0);
        r.params.insert("name".into(), json!("ball"));
        let mut w = half("w", "state/write", 1, 300.0);
        w.params.insert("name".into(), json!("ball"));
        let mut p = patch(vec![r, w], vec![]);
        // Duplicating the pair: the copies pair with each other, not the
        // originals.
        let new = duplicate(&mut p, &["r".into(), "w".into()], 24.0, 24.0);
        assert!(reslot_copies(&mut p, cat, &new).is_empty());
        assert_eq!(slot(&p, &new[0]), 2);
        assert_eq!(slot(&p, &new[1]), 2);
        assert_eq!(slot(&p, "w"), 1);
        assert_eq!(partners(&p, &new[1]), [new[0].clone()]);
        // A lone Write pasted: a fresh slot and a new Read, named alike.
        let clip = crate::editor::clipboard::copy(&p, &["w".into()]).unwrap();
        let pasted = crate::editor::clipboard::paste(&mut p, &clip, Position { x: 500.0, y: 0.0 });
        let added = reslot_copies(&mut p, cat, &pasted);
        assert_eq!(added.len(), 1);
        let read = find_node(&p, &added[0]).unwrap();
        assert_eq!(read.def_type, "state/read");
        assert_eq!(name(read), Some("ball"));
        assert_eq!(slot(&p, &pasted[0]), 3);
        assert_eq!(slot(&p, &added[0]), 3);
        // Every slot now has exactly one Write.
        for s in 1..=3 {
            let writes = p
                .nodes
                .iter()
                .filter(|n| n.def_type == "state/write" && n.params["slot"] == s)
                .count();
            assert_eq!(writes, 1, "slot {s}");
        }
    }

    #[test]
    fn the_single_node_state_loop_is_out_of_the_browser() {
        let cat = Catalogue::builtin();
        let found = crate::editor::search::search(cat, "state", &crate::editor::search::Fit::Any);
        let types: Vec<&str> = found.iter().map(|e| e.spec.ty.as_str()).collect();
        assert!(!types.contains(&"state/value"), "{types:?}");
        assert!(!types.contains(&"state/spread"));
        assert!(types.contains(&"state/write") && types.contains(&"state/read"));
        // Still loads.
        assert!(cat.get("state/value").is_some());
    }
}
