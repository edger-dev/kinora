---
# kinora-ml4t
title: 'Render: revisit branch labels under hot-ledger layout'
status: todo
type: task
priority: low
created_at: 2026-04-19T06:23:45Z
updated_at: 2026-04-19T06:24:00Z
parent: kinora-w7w0
---

Under the hot-ledger layout (kinora-xi21), new events live in `.kinora/hot/<ab>/<hash>.jsonl` and don't belong to any legacy per-lineage ledger file. But the render layer still groups pages under a branch label derived from the old `.kinora/ledger/<lineage>.jsonl` naming — on dogfood runs this shows up as the `7a155b58` label (the shard of RFC-0003's birth lineage) being applied to every page, including unrelated design-principle kinos.

Observed while completing kinora-ve9g. Not urgent — pages still render and read correctly — but the grouping UX is now misleading.

## Acceptance

- [ ] Decide what 'branch' means under hot-ledger (per-author? per-worktree? none?)
- [ ] Update `render` to derive labels from event metadata or drop the grouping when not meaningful
- [ ] Test covers both pure-hot and mixed hot+legacy repos

## Notes

Scope is UX/render-layer only; no data model changes expected.
