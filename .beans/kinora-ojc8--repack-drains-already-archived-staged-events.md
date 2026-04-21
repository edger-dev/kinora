---
# kinora-ojc8
title: Repack drains already-archived staged events
status: completed
type: bug
priority: normal
created_at: 2026-04-21T14:44:31Z
updated_at: 2026-04-21T15:04:56Z
---

Close the migration-debt gap from wcpp: repack should drain staged events whose hashes already exist in a commit-archive kino.

## Context

kinora-wcpp's drain fires only when `result.new_version.is_some()` (see
`commit.rs:771-783`). Repos with pre-wcpp staging — where events were
archived into `commit-archive` kinos but never drained (MaxAge roots
pre-wcpp were gated off) — can't self-clean via a no-op commit.

`clone_repo` (which repack invokes after commit) copies staged events
wholesale as long as their content blobs are reachable. Since the blobs
are reachable (archive kinos reference them, and the root kinograph
entries pin their versions), those stale staged events survive
the clone. Repack completes, `.kinora/staged/` stays populated.

## Scope

Add a post-commit, pre-clone drain pass inside `repack_repo` that
drops staged events whose hash already appears in a `commit-archive`
kino, for source roots with `RootPolicy::Never | MaxAge(_)`.

- [x] Add `drain_archived_orphans(kinora_root) -> Result<usize, CommitError>`
  in `commit.rs`:
  - Load config + commits root kinograph
  - For each commits-root entry of kind `commit-archive`, parse
    metadata `name` (`<source_root>-commit-archive`) to identify the
    source root
  - If `source_root`'s policy is `Never | MaxAge(_)`, read the archive
    blob from ContentStore, parse it, collect event hashes, and
    `drop_staged_events` those hashes
  - Tolerate missing `commits` pointer (fresh repos, empty archives)
- [x] Call it from `repack.rs::repack_repo` after `commit_all` and
  before `clone_repo`
- [x] Surface the drained-count in the `RepackReport` (new
  `orphan_events_drained: usize` field) and in CLI output
- [x] Tests:
  - [x] Migration-debt scenario: simulate pre-wcpp state by
    re-writing a staged event that's already in an archive;
    repack drains it
  - [x] No-op scenario: staged event not in any archive stays put
  - [x] Policy gate: KeepLastN source root's archived events are
    NOT drained (KeepLastN keeps its retention in staging)

## Acceptance

- [x] Tests added and passing
- [x] No regression of existing repack/commit tests
- [x] Zero compiler warnings

## Summary of Changes

Added a post-commit orphan-drain pass to `repack` that closes wcpp's migration-debt gap for pre-wcpp staged events that can't self-clean via a no-op commit.

### Implementation

- **`kinora/src/commit.rs`**: new public `drain_archived_orphans(kinora_root) -> Result<usize, CommitError>`. Walks the commits-root kinograph, picks entries of kind `commit-archive`, parses each archive blob, collects event hashes, and drops staged events for source roots whose policy is `Never` or `MaxAge(_)`. Tolerates missing commits pointer and reaped archive blobs (both return `Ok(0)` without failing).
- **`kinora/src/repack.rs`**: `repack_repo` now calls `drain_archived_orphans` between `commit_all` and `clone_repo`. New `RepackReport.orphan_events_drained: usize` field surfaces the drop count.
- **`kinora-cli/src/repack.rs`**: `format_repack_summary` prints `N orphan staged event(s) drained` line.

### Tests (6 unit + 1 e2e)

- `drain_archived_orphans_drops_staged_event_already_in_archive` — migration-debt case
- `drain_archived_orphans_preserves_unarchived_staged_events` — leaves unarchived events alone
- `drain_archived_orphans_respects_keep_last_n_policy` — `KeepLastN` source roots keep their retention
- `drain_archived_orphans_is_noop_when_commits_pointer_absent` — fresh repos
- `drain_archived_orphans_tolerates_missing_archive_blob` — reaped archive blob returns `Ok(0)`
- `drain_archived_orphans_spans_multiple_archive_entries` — iteration accumulates across all archive entries
- `repack_drains_orphan_archived_events_left_behind` — e2e through `repack_repo`

### Design decisions

- Drain runs post-commit so wcpp's commit-time drain handles the happy path; the pass is specifically for migration debt and repos where every subsequent commit is a no-op.
- Policy gate (`Never`/`MaxAge` only) matches wcpp's semantics — `KeepLastN` intentionally retains events in staging.
- Archive name suffix `-commit-archive` is the naming convention from `commit_archive.rs`; source root is extracted via `strip_suffix`.

### Results

- 519 tests pass, zero compiler warnings
- Commits: 78dd8e5 (tests), 77f2754 (impl), 9cbe743 (review fixes)
