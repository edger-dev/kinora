---
# kinora-7mou
title: 'Phase 3.5: compact consumes assigns + AmbiguousAssign + UnknownRoot errors'
status: completed
type: task
priority: normal
created_at: 2026-04-19T10:18:28Z
updated_at: 2026-04-19T12:11:37Z
parent: kinora-hxmw
blocked_by:
    - kinora-g08g
    - kinora-l79b
---

compact_root picks up live assigns; AmbiguousAssign (D2) and UnknownRoot (D4) errors.

Fifth piece of phase 3 (kinora-hxmw). Teaches `compact_root` to consume live `assign` events and enforces the two hard failures that the design (D2, D4) calls for. This is where the exclusive-ownership invariant becomes real.

## Scope

### In scope

- [x] During `compact_root(Y)`:
  - Read all events from the hot ledger (post-hxmw-2 generalized format).
  - Collect all `assign` events with `target_root == Y`.
  - Compute the **live set**: an assign is live iff no other assign event's `supersedes` list names its hash. Apply transitively (a superseder of a superseded assign is itself live).
  - For each live `assign → Y`, include the target kino's current head in root Y's next version.
  - For each live `assign → Y` where the kino was previously owned by root X (i.e., appeared in X's last-compacted pointer), remove it from X's next compaction — this means `compact_root(X)` must also see the `assign → Y` and drop the entry, not just `compact_root(Y)`. Document the cross-root visibility rule in the implementation.
- [x] Unassigned kinos (no live assign event ever) are implicitly routed to `inbox`. `compact_root("inbox")` picks them up as if they had a live `assign → inbox`.
- [x] `AmbiguousAssign { kino_id, candidates: Vec<AssignCandidate> }` error: raised by `compact_root` (hence bubbles through `compact_all` as a per-root error) when a kino has ≥2 live assigns. `AssignCandidate` carries `event_hash`, `target_root`, `author`, `ts` so the CLI can render the resolution hint from D2.
- [x] `UnknownRoot { name, event_hash }` error: raised by `compact_root` when a live assign references a target root not declared in `Config.roots`. Checked against the config loaded at compact_all time (pass the root name set into compact_root or have compact_root load the config itself — decide during implementation).
- [x] CLI output rendering for both errors matches the D2 mock-up:
  ```
  root=rfcs  ERROR: ambiguous assigns for kino aaaa…
    - assign → rfcs    (event abc1…, yj, 2026-04-19T10:00:00Z)
    - assign → designs (event def2…, yj, 2026-04-19T11:00:00Z)
  to resolve: kinora assign aaaa… <root> --resolves abc1…,def2…
  ```
- [x] Tests:
  - Single live `assign → rfcs` moves kino from default inbox bucket to rfcs root after compact.
  - Superseded assign is not live; superseder wins.
  - Transitively superseded: A superseded by B, B superseded by C → only C is live.
  - Two live competing assigns → `AmbiguousAssign` with both candidates.
  - Assign targeting undeclared root → `UnknownRoot` with the offending event hash.
  - Cross-root removal: kino previously in `main`, now assigned to `rfcs` → after next compact of both, appears only in rfcs' root version.

### Out of scope (deferred)

- GC/prune (hxmw-6)
- Cross-root composition integrity (hxmw-7)

## Acceptance

- [x] All sub-points under "In scope" implemented with tests
- [x] Zero compiler warnings
- [x] Bean todo items all checked off
- [x] Summary of Changes section added at completion


## Plan

### Semantics

For `compact_root(Y)` with Y any root name:

1. Read all hot events; split into store events and assign events (by `event_kind`).
2. Compute **live assigns**: drop every assign whose hash is named in some other assign's `parents` (supersedes). Apply transitively (a superseder of a superseded assign is itself live unless also superseded).
3. Group store events by `id`; pick head per id (existing logic).
4. For each (id, head):
   - Collect live assigns with `kino_id == id`.
   - 0 live assigns → route to `"inbox"`.
   - 1 live assign → route to that assign's `target_root`; if `target_root` ∉ declared roots, raise `UnknownRoot`.
   - ≥2 live assigns → raise `AmbiguousAssign { kino_id, candidates }`.
5. Include in root Y's entries iff routed target == Y.

Cross-root removal falls out for free: a kino previously in root X but now assigned to Y shows up only in Y after the next compact of both (because root content is always rebuilt from hot events, not inherited from prior root version).

### Types to add

```rust
pub struct AssignCandidate {
    pub event_hash: String,
    pub target_root: String,
    pub author: String,
    pub ts: String,
}

pub enum CompactError {
    // + existing
    Assign(AssignError),
    AmbiguousAssign { kino_id: String, candidates: Vec<AssignCandidate> },
    UnknownRoot { name: String, event_hash: String },
}
```

### API shape

- `build_root(events, root_name, declared_roots) -> Result<RootKinograph, CompactError>` (add two params).
- `compact_root` loads `config.styx` to get declared roots and passes them into `build_root`.
- `compact_all` unchanged at the surface — still just iterates declared roots and calls `compact_root`.

### Existing-test migration

Most tests today compact to `"main"` while the config only declares `"inbox"`. Under new semantics, unassigned kinos route to inbox so compacting to `"main"` yields empty. Migrate those tests to compact to `"inbox"` where they test plain mechanics (pointer writes, parent linkage, no-op, etc.). Tests that specifically exercise multi-root behavior stay on named roots and declare them explicitly.

### CLI rendering

`main.rs` special-cases the two new error variants to match D2:

```
root=rfcs  ERROR: ambiguous assigns for kino aaaa…
  - assign → rfcs    (event abc1…, yj, 2026-04-19T10:00:00Z)
  - assign → designs (event def2…, yj, 2026-04-19T11:00:00Z)
to resolve: kinora assign aaaa… <root> --resolves abc1…,def2…

root=main  ERROR: unknown root `madeup` referenced by assign event xyz…
```

### Commit sequence

1. `test(compact): 7mou routing semantics — failing tests`
2. `feat(compact): consume assigns; AmbiguousAssign + UnknownRoot; route to inbox by default`
3. `feat(cli): D2 rendering for AmbiguousAssign / UnknownRoot`
4. (optional) review-fix commit

## Summary of Changes

Three commits on branch `main`:

1. `34b0f64 test(compact): 7mou routing semantics — failing tests`
2. `5f52f3c feat(compact): consume assigns; route via live-assign graph`
3. `11a2d8e feat(cli): D2 rendering for AmbiguousAssign / UnknownRoot (7mou)`

### Library (`crates/kinora/src/compact.rs`)

- Added `AssignCandidate { event_hash, target_root, author, ts }` and new `CompactError` variants: `Assign(AssignError)`, `AmbiguousAssign { kino_id, candidates }`, `UnknownRoot { name, event_hash }`.
- New `collect_live_assigns` computes the live assign graph: parse all `event_kind == "assign"` entries via `AssignEvent::from_event`, then filter out every assign whose hash is named in some other assign's `supersedes` list. Transitive supersedes falls out because the `superseded` set is built over *all* assigns at once.
- New `kino_target_root` returns `Ok(None)` for the inbox default, `Ok(Some(name))` for a single live assign (validated against declared roots), `AmbiguousAssign` for ≥2 live, `UnknownRoot` for targets missing from `config.roots`.
- `build_root` now takes `(events, root_name, declared_roots: &BTreeSet<String>)`. Routing happens *before* `pick_head` so `MultipleHeads` only fires for kinos actually destined for the current root.
- `compact_root` loads `config.styx` once, materializes `declared_roots`, and passes it to `build_root`. No-op detection is now keyed on `root.entries.is_empty()` (routing output) rather than `events.is_empty()` (raw ledger) so declared-but-empty roots still emit a clean no-op line.
- `UnknownRoot` fires from *any* `compact_root` call that sees the offending assign, not just the one whose name matches — an undeclared target is treated as a global config error.

### CLI (`crates/kinora-cli/src/compact.rs`, `main.rs`)

- New `render_compact_entry` and `render_compact_error` render each `CompactAllEntry`. `AmbiguousAssign` emits the D2 multi-line block (candidate list with a target column padded to the widest root name, plus a copy-pasteable `kinora assign <id> <root> --resolves abc…,def…` hint). `UnknownRoot` emits a single line naming the offending event hash and undeclared target. Other variants fall through to `root=X ERROR: <display>`.
- `main.rs` drops its inline match and just loops `render_compact_entry` over each per-root entry.
- Five new CLI tests pin the exact rendered bytes for both variants plus fallback and Ok-branch regressions.

### Test fixture migrations

- Most library tests previously compacted to `"main"` while config only declared `"inbox"`. Under the new semantics unassigned kinos route to inbox, so a bulk rename (lines 570–910 of compact.rs) switched pointer-write / parent-linkage / no-op mechanics tests to `"inbox"`. Tests that genuinely exercise multi-root behavior declare `"main"` / `"rfcs"` / etc. explicitly in config and author assigns to route kinos there.
- `compact_ignores_non_store_events` switched its forged event to `event_kind: "future_kind"` so it slides past both `is_store_event()` and the assign parser.
- `compact_all_per_root_errors_do_not_short_circuit_clean_roots` now authors explicit assigns (`km → main`, `kc → clean`) so those roots materialize pointers while the `broken` root still errors.
- `crates/kinora-cli/src/render.rs` and `crates/kinora-cli/src/compact.rs` grew `declare_main_root` and `assign_to` helpers; four render tests and one CLI compact test now declare `main` and assign kinos to it before compact.

### Tests

All 287 library + 77 CLI tests pass, zero compiler warnings.

### Review

Fresh-eyes subagent review on `34b0f64..HEAD` found no blockers. One actionable doc fix applied: a short comment on `render_compact_error` documents the intentional double-space after `root=<name>` for the two structured variants (matches the D2 mock-up; flags them against the generic fallback).
