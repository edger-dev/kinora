---
# kinora-860i
title: 'CLI: store command'
status: completed
type: feature
priority: normal
created_at: 2026-04-18T09:16:59Z
updated_at: 2026-04-18T16:14:31Z
parent: kinora-w7w0
blocked_by:
    - kinora-5k13
---

Sketch → staged transition. Reads content (stdin or file), hashes with BLAKE3, writes to `.kinora/store/`, appends event to current lineage file.

RFC-0003 sections: *Minimal CLI → store*, *Kino Lifecycle*. Design decisions in `kinora-fhw1`.

## Design

### Command shape (tentative)

```
kinora store <kind> [path] \
  [--name NAME] \
  [--id ID] \
  [--parents HASH,HASH] \
  [--draft] \
  [--provenance TEXT] \
  [--metadata KEY=VALUE ...]
```

- `<kind>` required: `markdown`, `kinograph`, `text`, `binary`, or namespaced extension
- `[path]` reads content from file; omit for stdin
- `--id` omit for birth events; provide to append a version to an existing identity
- `--parents` comma-separated list of parent hashes (linear: 1; fork: 0; merge: 2+; detach/combine: cross-identity)

### Behavior

1. Read content from stdin or file
2. Compute BLAKE3 hash
3. Dedup: if hash exists in store, skip blob write; still append ledger entry
4. Validate: namespace rules on metadata keys, parents exist, kind is valid
5. Determine current lineage via `.kinora/HEAD`; mint new lineage if absent
6. Append JSONL event
7. Leave workspace ready for `git add` + `git commit`

### Id resolution

- Birth event (no `--id`, no `--parents`): `id = hash = BLAKE3(content)`, `parents = []`
- Version event (has `--id` or `--parents`): validated against existing history in the ledger

## Acceptance

- [x] Accepts content from stdin and file path
- [x] Records name (in metadata), kind, provenance, draft flag, timestamp
- [x] Provenance required (errors if missing)
- [x] Dedups store writes by hash; still appends ledger event
- [x] Validates namespace rules (reject unknown bare metadata keys)
- [x] Validates parent hashes exist in store
- [x] Mints new lineage file on first store in new branch
- [x] Appends to current lineage file when lineage exists
- [x] Updates `.kinora/HEAD` when minting new lineage
- [x] After running, workspace ready for `git add` + `git commit`

## Plan

Library-first. Put orchestration in `kinora` so it's unit-testable; CLI is a thin wrapper.

**`crates/kinora/src/`:**
- `author.rs` — `resolve_author_from_git(repo_root)` via gix → reads user.name from merged git config. Falls back to None.
- `kino.rs` — `StoreKinoParams` + `store_kino(kinora_root, params)` orchestrates: write blob (dedup), resolve id/parents (birth vs version), build Event, validate shape + parent-presence, mint or append to ledger. Returns `StoredKino { event, lineage, was_new_lineage }`.

**`crates/kinora-cli/src/`:**
- `cli.rs` — figue CLI struct with `store` subcommand (positional kind + optional path; flags --name/--id/--parents/--draft/--provenance/--author/--metadata/-m).
- `common.rs` — walk up from cwd to find `.kinora/`; error if not found.
- `store.rs` — read content from file|stdin, parse metadata k=v, parse comma-split parents, build params, resolve author, call library, print `kind id hash lineage`.
- `main.rs` — argv parse via `figue::from_std_args()`, dispatch on subcommand.

**Scope cuts:** 'first-store-on-new-branch mints new lineage' reduces to 'HEAD absent ⇒ mint, else append'. Branch-aware minting is post-MVP. Kinograph name→id resolution lives in kinora-zboo, not here — store just writes whatever bytes it receives.

Commit plan:
1. author + kino modules with tests
2. CLI wiring + integration test
3. Review fixes if any

## Progress

- [x] Library: `author::resolve_author_from_git` + `kino::store_kino` orchestrator.

## Summary of Changes

Wired `kinora store` end-to-end on top of the kinora-5k13 data layer.

**Library (`crates/kinora/src/`)**

- `author.rs` — `resolve_author_from_git(repo_root)` reads `user.name` from the repo's merged git config via gix; returns None for non-git dirs or when unset. Used as a default when `--author` is omitted.
- `kino.rs` — `store_kino()` orchestrates: write blob (dedup via ContentStore), decide birth vs version (birth = no id + no parents; `--parents` without `--id` is rejected with `ParentsWithoutId`), build the Event envelope, run `validate_event_shape` + `validate_parents_exist`, then mint or append to the ledger based on HEAD presence.

**CLI (`crates/kinora-cli/src/`)**

- `cli.rs` — figue-derived `Cli` + `Command::Store` with all spec flags (positional `kind` + `path`, required `--provenance`, optional `--name` / `--id` / `--parents` / `--author`, `--draft` switch, repeatable `-m key=value`). Flattens `FigueBuiltins` so `--help`, `--version`, and `--completions` just work.
- `common.rs` — walks up from cwd to find `.kinora/`, parses `KEY=VALUE` (trimmed key; empty key rejected; values may contain `=`), splits comma-delimited parents.
- `store.rs` — pure `StoreRunArgs → StoredKino` runner: reads content from file or stdin, assembles metadata (name, draft flag, repeatable -m), resolves author (flag > git user.name), stamps with jiff, dispatches to `store_kino`. Rejects `--draft` + `-m draft=X` collisions with `ConflictingDraftFlag` rather than letting flag order decide silently.
- `main.rs` — `figue::from_std_args → .into_result() → .get()`; exits 0 for help/version/completions via `DriverError::is_success()`, 1 for parse errors or runtime failures, printing `stored kind=… id=… hash=… lineage=…` on success.

**Design decisions documented inline**

- MVP lineage model: one lineage per repo. HEAD-absent mints, HEAD-present appends. Per-branch and per-identity lineage partitioning is deferred — identity lives in `event.id`, not the lineage filename. Noted in `kino.rs`.
- A CLI `init` subcommand is not in this task; library-level `init()` is complete. Follow-up bean will wire it into the CLI.

**Test coverage**

113 workspace tests (94 library + 19 CLI). Library tests cover birth, version, missing-parent rejection, unknown kind/metadata rejection, dedupe (no blob rewrite), and missing-.kinora. CLI tests cover file+stdin paths, draft→metadata, -m parsing, key trimming, invalid metadata rejection, draft-collision rejection, no-kinora error, author-unresolved, and version-event flow. Zero compiler warnings.

**Smoke-tested end-to-end** against a hand-laid-out `.kinora/`: birth→new lineage, second birth→append, version event with `--id`/`--parents`, `-m tags=a,b` metadata, stdin content, `--help` rendering.

**Follow-ups (separate beans)**

- Wire `kinora init` into the CLI.
- Branch-aware lineage minting (`git rev-parse` integration).
- Optional: warn when `--parents` contains stray empty entries (`,,abc`).
