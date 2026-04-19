---
# kinora-f0rg
title: 'Phase 3.7: cross-root integrity (external refs prevent GC drops)'
status: todo
type: task
priority: normal
created_at: 2026-04-19T10:19:10Z
updated_at: 2026-04-19T10:19:13Z
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
