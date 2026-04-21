---
# kinora-ou1u
title: 'Reformat: synthesize nested composition entries from pins'
status: todo
type: task
priority: low
created_at: 2026-04-21T14:36:18Z
updated_at: 2026-04-21T14:36:18Z
---

## Context

kinora-wcpp (completed 2026-04-21) widened the post-archive drain to
`MaxAge`, which means most kinograph store events in the default
`inbox` root (MaxAge 30d) are drained from staging after each commit.
They remain recoverable via the `commit-archive` kino.

wcpp patched `reformat_repo` Step 2 to synthesize store-event stubs
from **root kinograph entries** so `pick_head` can resolve archived
heads. That covers the common case: user runs `reformat` against a
repo where top-level entries sit in root kinographs.

**Gap:** nested composition entries — kinograph entries referenced by
another kinograph's `entries` list, that are NOT themselves root
kinograph entries — have no synthesis path. When `pick_head` tries
to resolve such an id, `events_by_id.get(&id)` returns None and the
entry is silently skipped (reformat.rs:~233-235).

Pre-existing for Never-policy roots, but now visible on every MaxAge
root (which is most of them in practice).

## Why This Is Low-Priority

- Reformat is idempotent and runs against a live repo. A subsequent
  `commit` cycle re-surfaces nested kinographs by materializing them
  as new kinograph store events (either via user edits or normal
  churn), at which point the next `reformat` run catches them.
- Legacy (.styx-wrapped) kinographs are only produced by pre-styxl
  versions of kinora. The population that needs this migration is
  fixed and small; once any given repo has reformatted its heads
  once, the gap only matters for never-reformatted nested kinographs.
- The drained event's content still exists in the archive kino; the
  information isn't lost, just not *reformatted* on the first pass.

## Scope

Close the gap by threading synthesis through composition entries:

- [ ] Extend `to_visit` (reformat.rs:201) to carry `(id, version)`
  pairs — the version comes from a root entry's `version` or a
  composition entry's `pin`.
- [ ] When visiting a nested entry, synthesize a stub into
  `events_by_id` using the pin (if present) as the version, read
  the content from ContentStore to determine kind, and fall through
  to the existing `pick_head`-free resolution path.
- [ ] When a composition entry has no pin AND its store event is
  neither staged nor in any root kinograph, emit a debug log and
  keep the silent skip (nothing to do).
- [ ] Add a test: nested kinograph with a drained store event whose
  parent is a root-entry kinograph — reformat should produce a new
  version for both the parent and the nested entry.

## Acceptance

- [ ] Test added and passing.
- [ ] No regression of existing reformat tests.
- [ ] Zero compiler warnings.
