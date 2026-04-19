---
# kinora-l79b
title: 'Phase 3.4: compact library split (compact_root / compact_all) + always-all CLI'
status: todo
type: task
priority: normal
created_at: 2026-04-19T10:18:01Z
updated_at: 2026-04-19T10:18:03Z
parent: kinora-hxmw
blocked_by:
    - kinora-c48l
---

Split compact into compact_root and compact_all; remove --root flag; per-root error isolation per D5.

Fourth piece of phase 3 (kinora-hxmw). Splits phase-2B's single `compact(kin_root, name, params)` into a per-root core (`compact_root`) and a batch driver (`compact_all`), and retires the `--root` flag from `kinora compact` per D5. No new compaction semantics land here — this is a refactor plus a CLI shape change that makes the downstream assign-consuming work (hxmw-5) testable in isolation.

## Scope

### In scope

- [ ] Rename library fn `compact` → `compact_root(kin_root: &Path, name: &str, params: CompactParams) -> Result<CompactResult, CompactError>`. Signature stays identical to today's `compact`.
- [ ] Add `compact_all(kin_root: &Path, params: CompactParams) -> Result<Vec<(String, Result<CompactResult, CompactError>)>, CompactError>`:
  - Loads `Config` from `config.styx`.
  - Iterates `config.roots` (sorted by name for deterministic output).
  - For each declared root, calls `compact_root` and collects the per-root result.
  - Per-root errors do not short-circuit the batch — clean roots still advance.
  - Outer `Result` only for pre-iteration failures (config read/parse).
- [ ] CLI: remove `--root` flag from `kinora compact`. Always calls `compact_all`.
- [ ] CLI output per D5: one line per root, e.g.
  ```
  root=main  version=<sh> (new version)
  root=rfcs  ERROR: <CompactError display>
  root=inbox version=<sh> (no-op)
  ```
- [ ] Exit code: `0` iff every root succeeded (new version OR no-op); `1` if any root errored. Clean roots still advance to disk even when exit is `1`.
- [ ] Tests (library): `compact_all` iterates all declared roots, per-root errors don't block clean roots, config load failure surfaces as outer `Err`.
- [ ] Tests (CLI): `kinora compact` without `--root` compacts every declared root; exit code logic; no-op roots print a no-op line; `--root` flag removal is backward-incompatible (explicit test asserting flag is no longer accepted).

### Out of scope (deferred)

- Assign consumption inside compact_root (hxmw-5)
- GC policy enforcement (hxmw-6)

## Acceptance

- [ ] All sub-points under "In scope" implemented with tests
- [ ] Zero compiler warnings
- [ ] Bean todo items all checked off
- [ ] Summary of Changes section added at completion
