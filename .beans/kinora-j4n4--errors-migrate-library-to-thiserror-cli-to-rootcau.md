---
# kinora-j4n4
title: 'Errors: migrate library to thiserror; CLI to rootcause'
status: completed
type: task
priority: normal
created_at: 2026-04-19T14:01:15Z
updated_at: 2026-04-19T14:25:54Z
---

Pairing thiserror (library, pure-typed) with rootcause (CLI, report/render) is the 80% win with minimal library impact. The library errors already implement `std::error::Error` manually and flatten inner errors into Display strings — thiserror's `#[from]` gives us proper `source()` chains for free.

## Scope

### Library — 15 enums across these files

- [x] `assign.rs` — AssignError
- [x] `compact.rs` — CompactError
- [x] `config.rs` — ConfigError
- [x] `event.rs` — EventError
- [x] `hash.rs` — HashParseError
- [x] `init.rs` — InitError
- [x] `kinograph.rs` — KinographError
- [x] `kino.rs` — StoreKinoError
- [x] `ledger.rs` — LedgerError
- [x] `namespace.rs` — NamespaceError
- [x] `render.rs` — RenderError
- [x] `resolve.rs` — ResolveError (careful: `MultipleHeads` carries custom fields the CLI pattern-matches)
- [x] `root.rs` — RootError
- [x] `store.rs` — StoreError
- [x] `validate.rs` — ValidationError

Each enum should keep the same Display output as today (tests may assert it). Use:
- `#[error("literal {field}")]` — Display with interpolation
- `#[error(transparent)]` + `#[from]` — pure wrappers
- `#[from]` — auto `From<E>` AND `source()`
- `#[source]` without `#[from]` — when we want `source()` but From would collide

### CLI

- [x] Add `rootcause` dep to `kinora-cli` (workspace already declares it).
- [x] Convert `CliError` to `thiserror`.
- [x] At command dispatch sites in `main.rs`, wrap the `CliError` in `rootcause::Report` and attach command-level context.
- [x] Replace `eprintln!("error: {e}")` → `eprintln!("{report}")` (Display form renders the tree with human-readable messages; {report:?} printed raw Debug of variants).
- [x] Preserve special-cased renderers (`MultipleHeads` fork report, `AmbiguousAssign` D2 hint).

## Plan

### Commit sequence

1. `refactor(errors): migrate library error types to thiserror` — mechanical sweep, zero behavior change, tests still pass.
2. `feat(cli): rootcause reports with per-command context` — replaces Display flattening with rootcause pretty output, attaches context at boundaries.

### Notes

- Some existing Display impls wrap the inner message with a prefix. With `#[error(transparent)]` that prefix disappears — CLI tests that check exact stderr strings need updating.
- `CompactError::AmbiguousAssign` and `ResolveError::MultipleHeads` are pattern-matched by the CLI for custom rendering; keep their shape.

## Acceptance

- [x] All existing tests pass (301 lib + 81 CLI)
- [x] Zero compiler warnings
- [x] `RUST_LOG` / `KINORA_TRACE` still work
- [x] CLI error output shows chained cause ≥1 level deep on a compound error
- [x] Bean todo items all checked off
- [x] Summary of Changes section added at completion

## Summary of Changes

**Commit 1 (`c22e79e`) — library error types on thiserror.**
Replaced hand-rolled `std::error::Error` + `Display` impls for all 15 library enums (AssignError, CompactError, ConfigError, EventError, HashParseError, InitError, KinographError, StoreKinoError, LedgerError, NamespaceError, RenderError, ResolveError, RootError, StoreError, ValidationError) with `#[derive(thiserror::Error)]`. Pure wrappers use `#[error(transparent)]` + `#[from]`; prefixed wrappers use `#[error("prefix: {0}")]` + `#[from]`; `PathBuf` fields use positional `#[error("... {}", .path.display())]`. Dropped ~420 lines of boilerplate. Display output byte-identical to the prior hand-rolled impls; `ResolveError::MultipleHeads` and `CompactError::AmbiguousAssign` field shapes preserved for CLI pattern-matching. Tests (301+81) unchanged.

**Commit 2 — CLI rootcause reports.**
Migrated `kinora-cli::common::CliError` to `thiserror` (17 variants). Added `rootcause` + `thiserror` deps to `kinora-cli/Cargo.toml`. Introduced a `report_err(command, e)` helper that wraps the error via `rootcause::Report::new_sendsync` with `.context(format!("`kinora {command}` failed"))` and prints `{report}` (Display form → tree layout with a location trail and the underlying error's Display at each node). All five command dispatches (store, assign, render, compact, resolve) route through this helper. The `ResolveError::MultipleHeads` arm still runs `render_fork_report` before the catch-all, so fork output is unchanged; `CompactError::AmbiguousAssign` is surfaced per-root by the existing `render_compact_entry` and is untouched by the top-level wrap.

**Chain verified.** With a corrupt `config.styx`, `kinora compact` now prints a two-node tree — the context node plus the underlying `ConfigError::Parse` Display — reachable via rootcause's `{report}` output. Further `source()` children surface when the inner error is itself a wrapper (e.g. `InvalidStoredHash { #[source] err: HashParseError }`).
