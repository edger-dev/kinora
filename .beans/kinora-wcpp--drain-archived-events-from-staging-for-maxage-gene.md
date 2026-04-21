---
# kinora-wcpp
title: Drain archived events from staging for MaxAge (generalize bayr)
status: todo
type: feature
priority: high
created_at: 2026-04-21T13:47:30Z
updated_at: 2026-04-21T13:50:40Z
---

## Context

kinora-q6bo introduced the commit-archive (staged events archived into a
root-specific `commit-archive` kino). kinora-bayr added post-archive staging
drain + `prior_root` merge in `build_root`, but gated both on
`RootPolicy::Never` only.

Result: roots configured with `MaxAge(duration)` (including the
auto-provisioned `inbox` root with `MaxAge("30d")`) still accumulate committed
events in `.kinora/staged/` indefinitely. Users running `kinora commit` +
`kinora repack` expect staging to be clean — but only Never-policy roots
benefit today.

The archive already preserves the data, so removing staged events after
archiving is pure deduplication, not data loss — this is what "commit"
conceptually means.

## Scope

Extend bayr's drain machinery to also fire for `MaxAge`:

- [ ] `commit_root_with_refs` — drop the `RootPolicy::Never` gate so
  `drain_archived_events_from_staging` also runs for `MaxAge`
- [ ] `build_root` — extend the `prior_root` merge path to `MaxAge` so old
  entries survive across commits and only age out via
  `apply_root_entry_gc`
- [ ] `prune_staged_events` — remove the `MaxAge` branch (retention now lives
  in the root kinograph via `apply_root_entry_gc`, not in staging)
- [ ] Keep `KeepLastN` untouched — its "N versions per kino" semantic is
  load-bearing on staging (root kinograph is one-entry-per-id). A separate
  bean can revisit this later if needed.

## Acceptance Criteria

- [ ] Tests added and passing:
  - [ ] `maxage_drains_archived_events_from_staging_after_commit`
  - [ ] `maxage_entries_age_out_of_root_kinograph_via_gc_post_drain`
  - [ ] `maxage_prior_root_merges_entries_across_commits`
  - [ ] Never-policy regression test remains green
  - [ ] `KeepLastN` regression test remains green (retention-via-staging
    preserved)
- [ ] Zero compiler warnings
- [ ] `kinora commit` + `kinora repack` on a default `inbox` root leaves
  `.kinora/staged/` empty (manual verify)
- [ ] Bean todo items all checked off
- [ ] Commits: tests / implementation / review-fixes (if any)
