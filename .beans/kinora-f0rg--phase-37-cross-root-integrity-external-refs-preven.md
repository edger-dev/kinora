---
# kinora-f0rg
title: 'Phase 3.7: cross-root integrity (external refs prevent GC drops)'
status: in-progress
type: task
priority: normal
created_at: 2026-04-19T10:19:10Z
updated_at: 2026-04-19T13:01:50Z
parent: kinora-hxmw
blocked_by:
    - kinora-mngq
---

If root A references (via composition) a kino owned by B, B's GC must not drop that version.

Final piece of phase 3 (kinora-hxmw). Enforces the xi21 invariant: a kino referenced by a composition kinograph (or any other kino) that is owned by a different root cannot be GC'd by its owning root. This makes composition across roots safe: you can depend on `rfcs/foo` from `main/bar` without worrying that rfcs' aggressive policy will silently break `main/bar`.

## Scope

### In scope

- [ ] Pre-GC pass in `compact_root(B)`:
  - Walk every other root's content (via their last-compacted root-kinograph blobs).
  - For each composition kinograph or referencing kino found there, collect the set of (id, version) pointers that live in root B.
  - Treat those pointers as implicit pins for this compaction — they survive GC even if policy would otherwise drop them.
- [ ] `compact_all` passes a shared "external references" set into each `compact_root` call so the O(N roots × N events) walk happens once per compact_all invocation, not per root.
- [ ] The check considers **composition** references (via `Kinograph` content), not just any hash that happens to appear in bytes. Only kinograph-kind kinos contribute references.
- [ ] When a cross-root reference saves an entry from GC, surface it in the compact CLI output so users know why something survived:
  ```
  root=rfcs  version=<sh> (new version; 2 entries retained by cross-root refs from main)
  ```
- [ ] Tests:
  - Root A (`never` policy) has a kinograph that composes kino X from root B (`30d` policy, X is 40 days old).
  - Without integrity check: B's GC would drop X.
  - With integrity check: X survives B's GC; the compact output mentions the retention.
  - Removing the reference (composing a different kino) and re-compacting: X is now eligible for GC again.
  - Circular reference (A references B which references A): no infinite loop; both roots' compacts complete.

### Out of scope (deferred)

- Non-composition reference tracking (future bean if needed)
- CLI flag to force-override the integrity check

## Acceptance

- [ ] All sub-points under "In scope" implemented with tests
- [ ] Zero compiler warnings
- [ ] Bean todo items all checked off
- [ ] Summary of Changes section added at completion

## Plan

### Semantics

**External reference:** a `(target_id, target_version)` pointer originating in a composition kinograph owned by some other root. `target_version` is either the `pin` field (when explicit) or the current head version of `target_id` (when the kinograph references the head).

**Cross-root integrity rule:** during `compact_root(B)`, any entry in root B whose `(id, version)` matches an external reference from another root is treated as **implicitly pinned** — it is protected from GC and its hot event is protected from prune, even if policy would otherwise drop it.

**Scope of "external":** only references from composition kinographs (i.e. `kind="kinograph"` entries in other roots). Arbitrary hash co-occurrences don't count. Root A referencing its own kino doesn't cross-protect itself (explicit pin already covers that case).

**When references are collected:** a single snapshot of every OTHER root's last-compacted root-kinograph at the start of the compaction. `compact_all` precomputes once; a standalone `compact_root(B)` call computes its own snapshot.

**Circular references:** handled naturally — the walk iterates root entries flatly; there's no recursive traversal. A refs B refs A: A's walk finds B's reference to A and vice versa.

**Concurrency:** external-ref snapshots are frozen at compact start. If root X compacts mid-batch and produces a new version, the snapshot is still based on X's pointer-at-start. This is conservative: B may retain entries that X no longer references after its compact, but never the reverse.

**Unpinned composition references:** when a kinograph `Entry` has empty `pin`, it "references the head". We resolve that to the current head version (via `pick_head`) of the referenced id at snapshot time. If resolution fails (no events, forked), we skip — the integrity check is best-effort, not a correctness gate.

### Types to add

```rust
/// (target_id, target_version) → set of referencing-root names.
/// Built once per compact_all invocation (or per standalone compact_root).
pub struct ExternalRefs {
    by_target: BTreeMap<(String, String), BTreeSet<String>>,
}
```

`CompactResult` gains `retained_by_cross_root: BTreeMap<String, usize>` (referencing-root → count) so the CLI can render the hint.

### API shape

- `ExternalRefs::collect(kinora_root, declared_roots, self_root, events) -> Result<Self, CompactError>`: walks every declared root other than `self_root`.
- `compact_root` internally computes `ExternalRefs` when not provided. An inner helper `compact_root_with_refs` takes the precomputed snapshot; `compact_all` uses that path.
- `apply_root_entry_gc` and `prune_hot_events` grow an `implicit_pinned_versions: &BTreeSet<String>` parameter. Entries/events matching these are protected identically to explicit pins.

### CLI rendering

`render_compact_entry` extends the Ok branch to append a parenthetical `(new version; N entries retained by cross-root refs from <root>[, <root2>])` when `retained_by_cross_root` is non-empty.

### Commit sequence

1. `test(compact): f0rg cross-root integrity — failing tests`
2. `feat(compact): external refs prevent cross-root GC drops`
3. `feat(cli): render cross-root retention hint on compact output`
4. (optional) review-fix commit
