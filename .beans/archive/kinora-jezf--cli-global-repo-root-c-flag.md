---
# kinora-jezf
title: 'CLI: global --repo-root / -C flag'
status: completed
type: task
priority: normal
created_at: 2026-04-19T14:50:35Z
updated_at: 2026-04-19T15:45:34Z
---

## Why

All commands currently start with `find_repo_root(cwd)` (see `kinora-cli/src/common.rs`), which walks up from the current directory looking for `.kinora/`. That's fine for interactive use but awkward for:

- Tests that drive the CLI against a tempdir
- Scripts operating on multiple repos
- The upcoming `clone` and `repack` commands, which need to target an arbitrary `.kinora/` path

A git-style `-C <path>` / `--repo-root <path>` flag solves all three cleanly.

## Semantics

- If `--repo-root` is given, use it verbatim as the repo root. Validate that `.kinora/` exists under it; error if not (same error type as `NotInKinoraRepo` today).
- If omitted, keep current behavior: walk up from cwd.
- Flag is global — accepted before or after the subcommand, visible on every subcommand's help.

## Areas affected

- `kinora-cli/src/cli.rs` — add the flag to the top-level `Cli` struct
- `kinora-cli/src/main.rs` — replace the `std::env::current_dir()` call with: if flag present use it, else use cwd, then still call `find_repo_root` on the result
- `kinora-cli/src/common.rs` — nothing to change; `find_repo_root` already takes a `&Path`

## Todos

- [x] Add `--repo-root` / `-C` to top-level CLI
- [x] Wire it into the path resolution in `run()`
- [x] Test: CLI against a tempdir via `-C` resolves correctly
- [x] Test: `-C /nonexistent` errors with `NotInKinoraRepo`
- [x] Update any existing docs/examples that assume cwd-only (none found)

## Acceptance

- Flag works on all subcommands (store, assign, render, compact, resolve)
- Zero warnings, all tests pass
- No behavior change when flag is absent

## Summary of Changes

Added global `-C` / `--repo-root` flag to the kinora CLI.

**Files changed:**
- `crates/kinora-cli/src/cli.rs` — added `repo_root: Option<String>` field with `-C` short alias on top-level `Cli` struct.
- `crates/kinora-cli/src/common.rs` — new `resolve_repo_root(cwd, override_path)` helper: verbatim when override is `Some` (errors if `.kinora/` isn't directly under it, no walk-up), walks up via `find_repo_root` when `None`. Four new unit tests cover both branches, plus the "doesn't silently walk up from a subdirectory of a real repo" edge case.
- `crates/kinora-cli/src/main.rs` — calls `resolve_repo_root` once in `run()` and shadows `cwd` with the result. Each `run_*` still takes `cwd` unchanged.

**Manual verification:**
- `kinora --help` shows the new flag on the top-level usage.
- `kinora -C /tmp/test commit` resolves to the tempdir (fails later on missing config, as expected).
- `kinora -C /nonexistent commit` errors with `not in a kinora repo: no .kinora/ found above /nonexistent`.

**Follow-ups noted in review (not blockers):**
- Each `run_*` still re-walks from the resolved root via `find_repo_root`. Harmless today (O(1) return), but the layering could be simplified by having `run_*` take an already-resolved root. Leaving as-is to keep the change minimal.
- Relative `-C subdir` paths flow through verbatim. Functional today, but downstream code should not assume the repo root is absolute.

All 386 workspace tests pass, zero compiler warnings.
