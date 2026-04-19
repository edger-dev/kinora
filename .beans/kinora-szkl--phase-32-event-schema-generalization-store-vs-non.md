---
# kinora-szkl
title: 'Phase 3.2: event schema generalization (store vs non-store events)'
status: in-progress
type: task
priority: normal
created_at: 2026-04-19T10:16:54Z
updated_at: 2026-04-19T10:57:00Z
parent: kinora-hxmw
---

Add event-kind discriminator to hot-ledger events; keep legacy store events parsing unchanged.

Second piece of phase 3 (kinora-hxmw). Today the hot-ledger `Event` struct assumes every event is a store event (content hash, kind-as-blob-kind, parents). Phase 3 needs multiple event kinds (`store`, `assign`, and future kinds like metadata resolution). This bean introduces the discriminator and refactors the consumers; no new event type lands here — that's hxmw-3.

## Scope

### In scope

- [ ] Refactor the on-disk hot event shape to include an event-kind discriminator distinct from the content `kind` field. Target shape (subject to review):
  - Add an `event_kind: "store" | "assign"` field (defaults to `"store"` when absent for backward-compat).
  - Alternatively: promote to a discriminated enum `HotEvent { Store(StoreEvent), Assign(AssignEvent) }` with a tag.
  - Decide during implementation; record the chosen shape in the Summary.
- [ ] Legacy hot files (no discriminator) parse as Store events.
- [ ] Update `Ledger::read_all_events` (and related callers) to return the generalized shape.
- [ ] Update consumers that iterate store-specific data (resolver, compact, render, validate) to filter for store events explicitly — `resolve` and `render` should silently ignore non-store events since they work off content.
- [ ] Event-hash computation stays deterministic per kind (canonical serialization includes the discriminator).
- [ ] `.jsonl` file path continues to be `<event-hash>` based; layout unchanged.
- [ ] Tests: round-trip a store event through the generalized code path, confirm legacy-format store events (no `event_kind` field) still parse, downstream consumers (resolver, render) ignore a hand-forged non-store event.

### Out of scope (deferred)

- The `assign` event type itself (hxmw-3)
- `kinora assign` / `kinora store --root` CLI (hxmw-3)
- Compaction consuming non-store events (hxmw-5)

## Acceptance

- [ ] All sub-points under "In scope" implemented with tests
- [ ] Zero compiler warnings
- [ ] Bean todo items all checked off
- [ ] Summary of Changes section added at completion (including the chosen on-disk shape)
