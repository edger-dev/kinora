---
# kinora-g08g
title: 'Phase 3.3: assign event + kinora assign CLI + store --root atomic pair'
status: completed
type: task
priority: normal
created_at: 2026-04-19T10:17:36Z
updated_at: 2026-04-19T11:37:12Z
parent: kinora-hxmw
blocked_by:
    - kinora-szkl
---

AssignEvent type, kinora assign command, kinora store --root birth-assign atomic pair.

Third piece of phase 3 (kinora-hxmw). Lands the user-visible surface for kino ownership: the `assign` event type, the `kinora assign` CLI, and the `kinora store --root <name>` birth-assign atomic pair. Does not yet teach compact to consume assigns — that's hxmw-5.

## Scope

### In scope

- [x] `AssignEvent { kino_id, target_root, supersedes: Vec<String>, author, ts, provenance, hash }` type with canonical serialization.
  - `supersedes` list may be empty (birth-time assign or first assign for a kino).
  - Event hash is computed over the canonical bytes, same discipline as store events.
- [x] Hot-ledger write path: assign events go to `.kinora/hot/<ab>/<event-hash>.jsonl`, one file per event. Use the event-kind discriminator introduced in hxmw-2.
- [x] `kinora assign <kino-or-name> <root> [--resolves <hashes,...>]` CLI subcommand:
  - Resolves the kino reference (id or `metadata.name`) via the existing resolver.
  - Populates `supersedes` from `--resolves`.
  - Does NOT validate that `<root>` exists in config — that's a compact-time check (hxmw-5).
  - Does NOT validate that superseded hashes exist — that's also compact-time.
  - Author resolved via existing author-flag / git-config precedence.
- [x] `kinora store --root <name>` flag:
  - Writes the birth/version event as today.
  - Immediately writes an `assign → <name>` event as an atomic pair.
  - Atomic pair: if either write fails, back out both (delete the written hot file).
  - `--root` without a value or an empty value is rejected at flag parse time.
  - `kinora store` without `--root` keeps today's behaviour — just the birth event. Compaction will route to inbox implicitly (hxmw-5).
- [x] Tests: assign event round-trip, `--resolves` populates supersedes, `--root` writes both events atomically, `--root` rollback on failure leaves no orphaned hot file, `kinora store` without `--root` still writes exactly one event.

### Out of scope (deferred)

- Compact consuming assigns / errors (hxmw-5)
- GC / policies (hxmw-6, hxmw-7)

## Acceptance

- [x] All sub-points under "In scope" implemented with tests
- [x] Zero compiler warnings
- [x] Bean todo items all checked off
- [x] Summary of Changes section added at completion

## Summary of Changes

### Phase A — library (`crates/kinora/src/assign.rs`)

- New `AssignEvent { kino_id, target_root, supersedes, author, ts, provenance }` struct. `event_hash()` computes the canonical JSON-line BLAKE3 hash via the shared `Event` serializer, so assign events share the store-event hashing discipline.
- `to_event()` / `from_event()` mappers onto the flat `Event` wire record — `event_kind = "assign"` (the discriminator from hxmw-2), `kind = "kin::assign"`, `id == hash == kino_id` as a placeholder (no content blob), `parents = supersedes`, metadata carries `kin::target_root`.
- `write_assign()` hot-ledger write path: validates kinora_root exists, kino_id/target_root non-empty, kino_id + each supersedes entry parse as `Hash`, then defers to `Ledger::write_event`. Returns `(event_hash, was_new)` with idempotent re-writes producing `was_new = false`.
- `AssignError` enum covering all validation paths (`KinoraMissing`, `NotAssignEvent`, `WrongKind`, `IdHashMismatch`, `InvalidHash`, `MissingTargetRoot`, `EmptyTargetRoot`, `EmptyKinoId`, `Event`, `Ledger`).
- `pub use crate::event::EVENT_KIND_ASSIGN` so downstream consumers can discriminate without reaching into `kinora::event`.
- 24 library tests covering: struct → Event round-trip, missing/mismatched fields, kind tag enforcement, target-root metadata enforcement, empty input rejection, invalid hash rejection, idempotent write, ledger file layout, supersedes → parents mapping.

### Phase B — CLI (`crates/kinora-cli/`)

- New `Assign` subcommand on `Cli` (positional `kino`, positional `root`, named `--resolves`, `--author`, `--provenance`).
- New `kinora assign` runner (`assign.rs`): resolves the kino reference via the standard resolver (id-form first if it parses as `Hash`, then name-form), parses `--resolves` into `supersedes`, falls back to git `user.name` for author, defaults `provenance = "assign"`, stamps `ts` from `jiff::Timestamp::now()`, then calls `write_assign`. Prints a one-line summary with `(new event)` suffix on first write.
- `kinora store --root <name>` flag on the existing store subcommand: writes the birth/version event via `store_kino` as today, then immediately writes a paired `assign → <name>` event. Empty `--root` is rejected before any write.
- `pair_assign_with_rollback()` helper: on assign-write failure, best-effort deletes the store event's hot file iff `was_new_lineage` is true, preserving the event-layer atomic-pair invariant. The content blob is intentionally not rolled back — the store is content-addressed and dedup-safe, so leaked blobs are benign and will be reaped by GC (hxmw-6). Documented inline.
- `CliError` extended with `EmptyRoot` and `Assign(AssignError)` variants.
- 16 new CLI tests including: assign by name, assign by id, event-kind in the ledger, `--resolves` wiring, empty-root rejection, unknown-kino error, outside-repo error, author fallback, default provenance, summary formatting; and on the store side: `--root` writes both events as a pair, empty `--root` rejected before any write, rollback on paired-assign failure leaves no orphaned store event hot file (using the directory-blocker trick to force `fs::rename` to fail), and the existing no-`--root` path still writes exactly one event.

### Commits

- `2fa276a` test(assign): AssignEvent module skeleton with failing tests (g08g)
- `b7cd5fe` feat(assign): implement AssignEvent wire mapping + write_assign
- `9a21efc` fix(assign): tighten from_event + write_assign validation (g08g review)
- `c50ac54` feat(cli): kinora assign + store --root atomic pair (g08g)
- `235148a` fix(cli): address g08g Phase B review nits

### Deferred

- Compact consuming assigns / erroring on unknown supersedes → **kinora-7mou** (hxmw-5)
- GC for leaked content blobs + superseded events → **kinora-mngq** (hxmw-6)
