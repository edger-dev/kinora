---
# kinora-61f9
title: 'Phase 2B: `kinora compact` command'
status: completed
type: feature
priority: normal
created_at: 2026-04-19T06:50:57Z
updated_at: 2026-04-19T07:15:13Z
parent: kinora-xi21
blocked_by:
    - kinora-h4xs
---

Phase 2B of the kinora-xi21 architecture. Implements manual compaction: read all hot events, derive a single flat root kinograph version deterministically, store it, and update the root pointer file.

Blocked by: kinora-h4xs (`root` kind + entry schema).

## Design

### Library surface

`crates/kinora/src/compact.rs`:

```
pub struct CompactResult {
    pub root_name: String,
    pub new_version: Option<Hash>,   # None when no-op
    pub prior_version: Option<Hash>,
}

pub fn compact(
    kinora_root: &Path,
    root_name: &str,
) -> Result<CompactResult, CompactError>;
```

### Algorithm

1. Read the current root pointer at `.kinora/roots/<name>` (if any → prior_version).
2. Read all hot events via `Ledger::read_all_events()`.
3. For each distinct kino identity, pick the head version — the event that is not a parent of any other event for the same id. (No merges mid-compaction — assign events come in phase 3.)
4. Build a `Kinograph` of kind `root` with one flat entry per head kino:
   - `id`, `version`, `kind` taken from the head event
   - `metadata` = the head event's metadata (as-is)
   - `note`, `pin` absent in phase 2 (added by phase 3's assign events)
5. Sort entries by id; serialize via canonical `to_styx()`.
6. If the prior root's content bytes are byte-identical → no-op, return `new_version = None`.
7. Otherwise, `store_kino` with `parents = prior_version.iter().collect()`, `id = None` if no prior (genesis) else `id = prior_root.id`.
8. Write the new version's content hash to `.kinora/roots/<name>` (tmp+rename).
9. Return `CompactResult { new_version: Some(h), … }`.

### Determinism

- Entries sorted by id (ascii-hex)
- Metadata keys sorted (already canonical from BTreeMap)
- When post-merge compaction has two prior root versions (left+right), `parents` lists them in canonical hash order — phase-2B covers this path but the test can be small since we're primarily exercising the genesis + single-parent happy path

### Default root name

Phase 2 ships single-flat-root. Default `--root main` when not specified. Phase 3 generalizes.

### CLI

`kinora compact [--root <name>]`:
- On success, print `root=<name> version=<sh> (new version)` if promoted; `root=<name> version=<sh> (no-op)` otherwise.
- Use the same author/provenance/ts resolution pattern as `kinora store` (git author + RFC3339 ts).

## Acceptance

- [x] `compact()` library fn: genesis case (no prior root) produces a root with `parents[]` empty
- [x] Subsequent compaction: `parents = [prior_version]`, new `version` hash differs
- [x] Idempotence: `compact` with no new events is a no-op (`new_version = None`), pointer file unchanged
- [x] Two independent compactions over the same hot-event set produce byte-identical root blobs (cross-dev determinism test)
- [x] Entry order is sorted by id — parse output and assert
- [x] Pointer file `.kinora/roots/<name>` contains the 64-hex version hash only (no trailing whitespace/newline, or explicit trailing newline — pick one and test it)
- [x] CLI `kinora compact` prints expected output, exits 0
- [x] Integration test: store 3 markdown kinos → compact → assert root has 3 entries → store a v2 → compact again → assert root has 3 entries with one bumped version
- [x] Zero compiler warnings

## Out of scope

- `assign` events / moving kinos between roots (phase 3)
- Multi-root / per-root policy / config.styx root declarations (phase 3)
- GC/prune (phase 3)
- Git hooks (explicitly deferred)
- Sub-kinograph entries (phase 4)


## Summary of Changes

Landed in 3 commits:

- `8e1be7e compact: library fn to promote hot events into root kinograph` — introduces `kinora::compact` module with `compact()`, `build_root()`, `read_root_pointer()`, `CompactParams`, `CompactResult`, `CompactError`. Adds `paths::roots_dir` / `paths::root_pointer_path` helpers.
- `c9d4e19 cli: kinora compact command` — wires `kinora compact [--root NAME] [--author NAME] [--provenance STR]` with git-resolved author and RFC3339 `jiff::Timestamp::now()` timestamp.
- `50a5459 compact: review fixes — distinct NoHead + validate root_name` — splits the zero-heads cycle case into a dedicated `NoHead` error (previously mis-shaped as `MultipleHeads { heads: [] }`) and adds a path-traversal guard on root name.

### Key design choices

- **Root-kind events are excluded from the new root's entries** — a root kinograph is the state of user content, not its own history. Prior root versions chain through the event `parents` link, not via self-reference.
- **Pointer file format**: exactly the 64-hex version hash, no trailing newline. `read_root_pointer` is forgiving of trailing `
`/`\r
` in case a human edited the file.
- **Determinism**: `build_root` groups events via `BTreeMap` (sorted by id), picks the single head per identity deterministically, and `RootKinograph::to_styx()` sorts entries by id. Two devs running compact over the same hot-event set produce byte-identical root blobs regardless of iteration order — verified by `two_independent_compactions_produce_byte_identical_root_blobs`.
- **No-op detection**: two paths — (a) no prior pointer + zero events → skip; (b) prior pointer exists and fresh canonical bytes match the stored prior bytes → skip. In both cases `new_version = None` is returned and the pointer file is untouched.
- **Forks rejected**: multiple heads for one identity error out as `MultipleHeads`. Phase 3's assign events will be the supported way to nominate a winner.
- **Multi-parent root linking** is a future path (post-merge reconciliation where two prior root versions need joining). Current implementation always emits `parents = vec![prior]` for the single-parent happy path — per the bean's design note that phase 2B primarily exercises genesis + single-parent.

### Tests (14 new, all passing)

- Genesis, subsequent, idempotence, cross-dev determinism, entry sort, pointer-file format, CLI defaults (`main`/`compact`/git author), 3-kino + v2-bump scenario, fork rejection, cycle (`NoHead`), invalid root name validation, pointer file trailing-newline tolerance.

Zero compiler warnings. All 228 kinora + 45 CLI tests pass.
