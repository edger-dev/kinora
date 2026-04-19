---
# kinora-2t6l
title: 'Rename: hot → staged, compact → commit'
status: completed
type: task
priority: normal
created_at: 2026-04-19T14:39:05Z
updated_at: 2026-04-19T15:37:50Z
---

## Why

'Hot' and 'compact' are jargon carried over from an earlier mental model. 'Staged' and 'commit' map directly onto the git vocabulary most users already carry, and they describe the behavior (assign events sit 'staged' waiting to become a 'commit' into the root-kinograph) with less cognitive load.

## Scope

Pure rename only — **no** lifecycle change in this bean. Staged events remain in place after commit, exactly as 'hot' events remain after compact today. Cleanup + history preservation is tracked separately.

## Areas affected

- `kinora/src/paths.rs` — `hot_dir`, `HOT_DIR` → `staged_dir`, `STAGED_DIR`
- `kinora/src/compact.rs` → `kinora/src/commit.rs` (module, types, functions)
- `kinora/src/ledger.rs` — any 'hot' naming
- `kinora-cli` — `compact` subcommand → `commit`; `--hot` flag names
- Tests — fixture paths, assertions referencing `.kinora/hot/`
- Docs — README, RFC-0003, CLAUDE.md
- Error types — `CompactError` → `CommitError`; variant names

## Todos

- [x] Rename `HOT_DIR`/`hot_dir` → `STAGED_DIR`/`staged_dir` in paths.rs
- [x] Rename module `compact` → `commit` in kinora lib
- [x] Rename `CompactError` → `CommitError`
- [x] Rename `run_compact`/`CompactReport` etc.
- [x] Rename CLI subcommand `compact` → `commit` (and its module in kinora-cli)
- [x] Update all tests that reference `.kinora/hot/` path literal
- [x] Update README, RFC-0003, CLAUDE.md references (no references found — N/A)
- [x] Verify zero warnings, all tests pass (382 tests pass, zero diagnostics)

## Acceptance

- `cargo test --workspace` passes
- Zero compiler warnings
- `kinora commit` works identically to the old `kinora compact`
- No residual `hot`/`compact` identifier in library code (hard rename, no transitional aliases — repo is days old, no external users)

## Summary of Changes

Pure mechanical rename across the Rust workspace:

- **Paths**: `HOT_DIR` → `STAGED_DIR`, `hot_dir()` → `staged_dir()`, `hot_event_path()` → `staged_event_path()`, string literal value `"hot"` → `"staged"`.
- **Module/files**: `compact.rs` → `commit.rs` in both `kinora` and `kinora-cli` crates (including module declarations in `lib.rs` and `main.rs`).
- **Types**: `CompactError` → `CommitError`, `CompactParams` → `CommitParams`, `CompactResult` → `CommitResult`, `CompactAllEntry` → `CommitAllEntry`, `CompactRunArgs` → `CommitRunArgs`, `CompactRunReport` → `CommitRunReport`. All error variants preserved.
- **Functions**: `run_compact` → `run_commit`, `compact_all` → `commit_all`, `compact_root` → `commit_root`, `render_compact_entry` → `render_commit_entry`, `render_compact_error` → `render_commit_error`.
- **CLI**: subcommand `compact` → `commit` (`Command::Compact` variant → `Command::Commit`).
- **Dogfood repo**: renamed `.kinora/hot/` → `.kinora/staged/` in place (`git mv` preserved all staged event files).

Execution: three sed passes (word-boundary, compound identifiers, unbounded), then fixed one false-positive rename (`snapshot` → `snapsstaged` → restored to `snapshot`), then renamed a doubled test name (`staged_dir_is_staged_subdir` → `staged_dir_is_staged_subdir_of_kinora_root`).

No README, CLAUDE.md, or RFC references needed updating (none found).

Result: 382 tests pass, zero compiler warnings, independent subagent review found no missed references, mangled substrings, or doc-comment incoherence. Commit `3e1a0ae` (approx) on main.
