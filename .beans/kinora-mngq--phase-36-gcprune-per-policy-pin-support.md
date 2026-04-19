---
# kinora-mngq
title: 'Phase 3.6: GC/prune per policy + pin support'
status: todo
type: task
priority: normal
created_at: 2026-04-19T10:18:49Z
updated_at: 2026-04-19T10:18:53Z
parent: kinora-hxmw
blocked_by:
    - kinora-c48l
    - kinora-l79b
---

Each root compaction prunes hot events and old entries per its declared policy; pin exempts.

Sixth piece of phase 3 (kinora-hxmw). Enforces the per-root retention policies declared in config (hxmw-1) during compaction, and adds the `pin: true` escape hatch for entries that must survive GC regardless of age or depth.

## Scope

### In scope

- [ ] Policy evaluation during `compact_root`:
  - `RootPolicy::Never` → no entry is pruned, no hot event is dropped.
  - `RootPolicy::MaxAge(duration)` → drop entries whose content version `ts` is older than `now() - duration`; prune hot events older than `now() - duration` from `.kinora/hot/`.
  - `RootPolicy::KeepLastN(n)` → keep only the N most recent content versions per kino id (by `ts`); older versions are candidates for drop.
- [ ] `pin: true` on a `RootEntry` exempts that entry (and the content version it names) from all GC. Pin is set via direct root-kinograph editing today; a CLI to toggle pin is deferred.
- [ ] Hot-ledger pruning: once the event is older than the root's policy AND the compaction that made it permanent has happened, the event file can be deleted. Implementation must not race with concurrent reads — use the existing atomic write discipline.
- [ ] Multi-version preservation: if a pin points at version N, versions N-1 and later still live in the store (since content is immutable and dedup'd). Policy only controls what the root kinograph names, not what the content store holds — document this distinction.
- [ ] Tests:
  - `Never` drops nothing even when entries are years old.
  - `MaxAge("7d")` drops an 8-day-old unpinned entry but keeps a 6-day-old entry.
  - `MaxAge("7d")` + pin → entry survives regardless of age.
  - `KeepLastN(3)` with 5 versions keeps exactly the 3 newest by ts.
  - `KeepLastN(3)` + pin on version 1 → that version survives in addition to the 3 newest (pin is non-exclusive with N-window).
  - Hot-ledger events older than policy are removed from disk after compact; fresh events untouched.

### Out of scope (deferred)

- Cross-root integrity (hxmw-7)
- CLI for toggling pin (future bean)

## Acceptance

- [ ] All sub-points under "In scope" implemented with tests
- [ ] Zero compiler warnings
- [ ] Bean todo items all checked off
- [ ] Summary of Changes section added at completion
