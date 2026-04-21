---
# kinora-0sgr
title: Record head_ts on RootEntry so GC is independent of staging
status: in-progress
type: feature
priority: high
created_at: 2026-04-21T13:50:34Z
updated_at: 2026-04-21T13:56:05Z
blocking:
    - kinora-wcpp
---

## Context

`apply_root_entry_gc` (commit.rs:991) ages out MaxAge kinograph entries by
looking up the head store event's `ts` in the staged event stream. Today this
works because MaxAge roots keep owned store events in staging as the retention
signal — entries remain in the kinograph until their staged head ages out,
which triggers both staging prune and entry drop in the same commit.

kinora-wcpp wants to drain MaxAge staged events eagerly after archiving
(deduplication — data is preserved in the archive kino). But that removes the
head event GC relies on. With the current fallback ("keep the entry when head
missing"), drained MaxAge entries would never age out — a regression, not an
improvement.

Fix: record the head event's ts directly on `RootEntry` at commit time, and
have GC read it from the entry instead of the staged stream. This decouples
retention semantics from staging, unblocking wcpp.

## Scope

- [x] Add `head_ts: String` field on `RootEntry` with `#[facet(default)]` for
  backward-compatible kinograph parsing (older blobs parse with empty ts)
- [ ] `build_root` populates `head_ts` from the picked head store event
- [x] `RootEntry::new` takes `head_ts` as a constructor arg (caller-updated)
- [ ] `apply_root_entry_gc` reads `entry.head_ts` instead of looking up events
- [ ] Empty `head_ts` (legacy entry): keep the entry (matches current
  conservative fallback)
- [ ] `propagate_pins` and `prior_root` merge: entries are copied whole,
  `head_ts` propagates naturally — no change needed

## Out of Scope

- Staging drain behavior itself — that's kinora-wcpp
- Changing the "pin exempts entry" rule
- Changing the retention unit (still seconds, still measured against head ts)

## Acceptance Criteria

- [ ] Tests added and passing:
  - [ ] `build_root_populates_head_ts_from_head_event`
  - [ ] `entry_gc_uses_head_ts_on_entry_not_staged_event` — simulated by
    building a root with known head_ts, then running GC with empty events
  - [ ] `entry_gc_keeps_entry_when_head_ts_is_empty` — legacy path
  - [ ] Existing MaxAge/KeepLastN/Never tests still pass
- [ ] Zero compiler warnings
- [ ] `kinora commit` behavior unchanged end-to-end (no observable diff
  without wcpp)
- [ ] Bean todo items all checked off
- [ ] Commits: tests / implementation / review-fixes
