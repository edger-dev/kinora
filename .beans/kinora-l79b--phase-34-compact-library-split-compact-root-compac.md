---
# kinora-l79b
title: 'Phase 3.4: compact library split (compact_root / compact_all) + always-all CLI'
status: completed
type: task
priority: normal
created_at: 2026-04-19T10:18:01Z
updated_at: 2026-04-19T11:48:35Z
parent: kinora-hxmw
blocked_by:
    - kinora-c48l
---

Split compact into compact_root and compact_all; remove --root flag; per-root error isolation per D5.

Fourth piece of phase 3 (kinora-hxmw). Splits phase-2B's single `compact(kin_root, name, params)` into a per-root core (`compact_root`) and a batch driver (`compact_all`), and retires the `--root` flag from `kinora compact` per D5. No new compaction semantics land here — this is a refactor plus a CLI shape change that makes the downstream assign-consuming work (hxmw-5) testable in isolation.

## Scope

### In scope

- [x] Rename library fn `compact` → `compact_root(kin_root: &Path, name: &str, params: CompactParams) -> Result<CompactResult, CompactError>`. Signature stays identical to today's `compact`.
- [x] Add `compact_all(kin_root: &Path, params: CompactParams) -> Result<Vec<(String, Result<CompactResult, CompactError>)>, CompactError>`:
  - Loads `Config` from `config.styx`.
  - Iterates `config.roots` (sorted by name for deterministic output).
  - For each declared root, calls `compact_root` and collects the per-root result.
  - Per-root errors do not short-circuit the batch — clean roots still advance.
  - Outer `Result` only for pre-iteration failures (config read/parse).
- [x] CLI: remove `--root` flag from `kinora compact`. Always calls `compact_all`.
- [x] CLI output per D5: one line per root, e.g.
  ```
  root=main  version=<sh> (new version)
  root=rfcs  ERROR: <CompactError display>
  root=inbox version=<sh> (no-op)
  ```
- [x] Exit code: `0` iff every root succeeded (new version OR no-op); `1` if any root errored. Clean roots still advance to disk even when exit is `1`.
- [x] Tests (library): `compact_all` iterates all declared roots, per-root errors don't block clean roots, config load failure surfaces as outer `Err`.
- [x] Tests (CLI): `kinora compact` without `--root` compacts every declared root; exit code logic; no-op roots print a no-op line; `--root` flag removal is backward-incompatible (explicit test asserting flag is no longer accepted).

### Out of scope (deferred)

- Assign consumption inside compact_root (hxmw-5)
- GC policy enforcement (hxmw-6)

## Acceptance

- [x] All sub-points under "In scope" implemented with tests
- [x] Zero compiler warnings
- [x] Bean todo items all checked off
- [x] Summary of Changes section added at completion


## Summary of Changes

Split the compact library into a per-root core and a batch driver, and retired
the `--root` flag from `kinora compact` per D5. No compaction semantics change
here — this is a refactor plus a CLI shape change that unblocks hxmw-5.

### Library (`crates/kinora/src/compact.rs`)

- Renamed `compact` → `compact_root(kin_root, name, params) -> Result<CompactResult, CompactError>`. Signature is byte-identical to the old `compact`.
- Added `compact_all(kin_root, params) -> Result<Vec<CompactAllEntry>, CompactError>` where `CompactAllEntry = (String, Result<CompactResult, CompactError>)`. It loads `config.styx`, iterates `config.roots` in name order (BTreeMap → sorted), and drives `compact_root` per root. Per-root errors land in the entry, not the outer `Err` — clean roots still advance to disk.
- Added `CompactError::Config(ConfigError)` + `From<ConfigError>` for the pre-iteration load path; outer `Err` is reserved for config read/parse failures.
- Five new tests: name-order iteration, per-root error isolation (no short-circuit), config-parse failure → outer `Err`, missing config file → outer `Err`, and no-op entry when a root has nothing to promote.

### CLI (`crates/kinora-cli/src/compact.rs`, `cli.rs`, `main.rs`)

- Removed `root: Option<String>` from the `Compact` variant and from `CompactRunArgs`. Dropped `DEFAULT_ROOT_NAME`; `DEFAULT_PROVENANCE` kept.
- `run_compact` always calls `compact_all` and returns `CompactRunReport { per_root: Vec<CompactAllEntry> }` with `any_error()` helper.
- `main.rs` prints one line per root: `root=<name> version=<sh> (new version)`, `root=<name> version=<sh> (no-op)`, or `root=<name> ERROR: <e>`. Exit is `FAILURE` iff `any_error()`, otherwise `SUCCESS` — clean roots still advance even when the process exits non-zero.
- Four new CLI tests: compact every declared root without `--root`, `any_error` flips when one root fails, no-op line when nothing to promote, and a figue-level test asserting `--root` on `compact` is now rejected as unknown.

### Render (`crates/kinora-cli/src/render.rs`)

- Four test-only call sites updated from `compact(...)` → `compact_root(...)` to track the rename. No production code change.

### Commits

- `eaf6901` feat(compact): split into compact_root + compact_all; retire --root (l79b)

Tests: 351 passing across the workspace; zero warnings. Code review by subagent found no must-fix / should-fix issues.
