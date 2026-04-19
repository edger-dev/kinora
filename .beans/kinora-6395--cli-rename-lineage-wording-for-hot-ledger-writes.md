---
# kinora-6395
title: 'CLI: rename `lineage=…` wording for hot-ledger writes'
status: todo
type: task
priority: low
created_at: 2026-04-19T06:23:57Z
updated_at: 2026-04-19T06:24:00Z
parent: kinora-w7w0
---

On every successful `kinora store`, the CLI prints something like:

    lineage=a1b2c3d4 (new lineage)

Under the hot-ledger layout, each event lives in its own file keyed by the event hash — there is no 'lineage file' anymore, and every hot write is trivially 'new'. The message is a carryover from the old per-lineage ledger layout and is now misleading:

- Re-storing the same logical event (idempotent no-op) still prints `(new lineage)` if the shorthash happens to differ from a prior run's print — but actually, on idempotent re-store it now prints nothing of note since we suppress; but on any new event (even a version under an existing identity) we say 'new lineage' even though the identity is unchanged
- New users map 'lineage' to a branch-like concept; the shorthash is really just the event hash's prefix

Observed while completing kinora-ve9g.

## Proposal (sketch)

- Rename the printed field to `event` (or `eh` for 'event hash') — e.g. `event=a1b2c3d4`
- Drop `(new lineage)`; use `(new event)` when `was_new_lineage=true` and omit otherwise
- Keep the `StoredKino.lineage` field name as-is for one release so programmatic callers aren't broken — document the deprecation in code

## Acceptance

- [ ] CLI print updated
- [ ] Integration/unit test asserts the new wording
- [ ] Docs/README reflects the new phrasing if it appears anywhere
