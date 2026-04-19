---
# kinora-7mou
title: 'Phase 3.5: compact consumes assigns + AmbiguousAssign + UnknownRoot errors'
status: todo
type: task
priority: normal
created_at: 2026-04-19T10:18:28Z
updated_at: 2026-04-19T10:18:31Z
parent: kinora-hxmw
blocked_by:
    - kinora-g08g
    - kinora-l79b
---

compact_root picks up live assigns; AmbiguousAssign (D2) and UnknownRoot (D4) errors.

Fifth piece of phase 3 (kinora-hxmw). Teaches `compact_root` to consume live `assign` events and enforces the two hard failures that the design (D2, D4) calls for. This is where the exclusive-ownership invariant becomes real.

## Scope

### In scope

- [ ] During `compact_root(Y)`:
  - Read all events from the hot ledger (post-hxmw-2 generalized format).
  - Collect all `assign` events with `target_root == Y`.
  - Compute the **live set**: an assign is live iff no other assign event's `supersedes` list names its hash. Apply transitively (a superseder of a superseded assign is itself live).
  - For each live `assign → Y`, include the target kino's current head in root Y's next version.
  - For each live `assign → Y` where the kino was previously owned by root X (i.e., appeared in X's last-compacted pointer), remove it from X's next compaction — this means `compact_root(X)` must also see the `assign → Y` and drop the entry, not just `compact_root(Y)`. Document the cross-root visibility rule in the implementation.
- [ ] Unassigned kinos (no live assign event ever) are implicitly routed to `inbox`. `compact_root("inbox")` picks them up as if they had a live `assign → inbox`.
- [ ] `AmbiguousAssign { kino_id, candidates: Vec<AssignCandidate> }` error: raised by `compact_root` (hence bubbles through `compact_all` as a per-root error) when a kino has ≥2 live assigns. `AssignCandidate` carries `event_hash`, `target_root`, `author`, `ts` so the CLI can render the resolution hint from D2.
- [ ] `UnknownRoot { name, event_hash }` error: raised by `compact_root` when a live assign references a target root not declared in `Config.roots`. Checked against the config loaded at compact_all time (pass the root name set into compact_root or have compact_root load the config itself — decide during implementation).
- [ ] CLI output rendering for both errors matches the D2 mock-up:
  ```
  root=rfcs  ERROR: ambiguous assigns for kino aaaa…
    - assign → rfcs    (event abc1…, yj, 2026-04-19T10:00:00Z)
    - assign → designs (event def2…, yj, 2026-04-19T11:00:00Z)
  to resolve: kinora assign aaaa… <root> --resolves abc1…,def2…
  ```
- [ ] Tests:
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

- [ ] All sub-points under "In scope" implemented with tests
- [ ] Zero compiler warnings
- [ ] Bean todo items all checked off
- [ ] Summary of Changes section added at completion
