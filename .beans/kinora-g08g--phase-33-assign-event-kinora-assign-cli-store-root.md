---
# kinora-g08g
title: 'Phase 3.3: assign event + kinora assign CLI + store --root atomic pair'
status: in-progress
type: task
priority: normal
created_at: 2026-04-19T10:17:36Z
updated_at: 2026-04-19T11:08:46Z
parent: kinora-hxmw
blocked_by:
    - kinora-szkl
---

AssignEvent type, kinora assign command, kinora store --root birth-assign atomic pair.

Third piece of phase 3 (kinora-hxmw). Lands the user-visible surface for kino ownership: the `assign` event type, the `kinora assign` CLI, and the `kinora store --root <name>` birth-assign atomic pair. Does not yet teach compact to consume assigns — that's hxmw-5.

## Scope

### In scope

- [ ] `AssignEvent { kino_id, target_root, supersedes: Vec<String>, author, ts, provenance, hash }` type with canonical serialization.
  - `supersedes` list may be empty (birth-time assign or first assign for a kino).
  - Event hash is computed over the canonical bytes, same discipline as store events.
- [ ] Hot-ledger write path: assign events go to `.kinora/hot/<ab>/<event-hash>.jsonl`, one file per event. Use the event-kind discriminator introduced in hxmw-2.
- [ ] `kinora assign <kino-or-name> <root> [--resolves <hashes,...>]` CLI subcommand:
  - Resolves the kino reference (id or `metadata.name`) via the existing resolver.
  - Populates `supersedes` from `--resolves`.
  - Does NOT validate that `<root>` exists in config — that's a compact-time check (hxmw-5).
  - Does NOT validate that superseded hashes exist — that's also compact-time.
  - Author resolved via existing author-flag / git-config precedence.
- [ ] `kinora store --root <name>` flag:
  - Writes the birth/version event as today.
  - Immediately writes an `assign → <name>` event as an atomic pair.
  - Atomic pair: if either write fails, back out both (delete the written hot file).
  - `--root` without a value or an empty value is rejected at flag parse time.
  - `kinora store` without `--root` keeps today's behaviour — just the birth event. Compaction will route to inbox implicitly (hxmw-5).
- [ ] Tests: assign event round-trip, `--resolves` populates supersedes, `--root` writes both events atomically, `--root` rollback on failure leaves no orphaned hot file, `kinora store` without `--root` still writes exactly one event.

### Out of scope (deferred)

- Compact consuming assigns / errors (hxmw-5)
- GC / policies (hxmw-6, hxmw-7)

## Acceptance

- [ ] All sub-points under "In scope" implemented with tests
- [ ] Zero compiler warnings
- [ ] Bean todo items all checked off
- [ ] Summary of Changes section added at completion
