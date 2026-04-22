---
# kinora-ve9g
title: 'Dogfood: capture design principles as kinos'
status: completed
type: task
priority: normal
created_at: 2026-04-19T06:07:36Z
updated_at: 2026-04-19T06:11:51Z
---

Break the design principles we have so far — store layout, hot ledger + branch handling, event envelope, kinograph composition, etc. — into individual kinos stored in this repo. Each kino should be a focused, self-contained explanation of one design choice. Compose them into a kinograph so 'render' produces a readable principles site.

## Plan

One kino per design decision (focused, composable). Stored as `kind=markdown` with `kind=design-principle` metadata tag so the kinograph can pick them up.

### Target kinos

1. **store-content-addressed** — BLAKE3 + shard + dedup
2. **hot-ledger-one-file-per-event** — why one file per event, and what it buys us across branches
3. **event-envelope** — the JSONL event shape and each field's purpose
4. **identity-and-versions** — identity = birth hash; versions linked via `parents[]`
5. **kinograph-composition** — composition by reference, not by copy
6. **name-vs-id** — id is authoritative; name is a reversible hint
7. **provenance-mandatory** — every kino records origin; why
8. **metadata-namespaces** — bare = reserved, `prefix::` = extension
9. **branch-merge-safety** — set-union of content-addressed files = no JSONL conflicts
10. **future-root-kinograph-chains** — self-hosted history, design direction (not yet built)

### Steps

- [x] Draft and store each kino via `kinora store markdown --name <slug> -m kind=design-principle`
- [x] Compose them into a `design-principles` kinograph
- [x] Render and sanity-check with mdbook
- [x] Commit `.kinora/` changes

## Summary of Changes

Authored 11 design-principle kinos (each a single markdown file) and composed them into a `design-principles` kinograph. Stored in this repo under `.kinora/` — all via the public `kinora store` CLI, exercising both markdown and kinograph kinds.

**Kinos (name → title):**

1. `store-content-addressed` — Content store is content-addressed
2. `hot-ledger-one-file-per-event` — Hot ledger: one file per event
3. `event-envelope` — The event envelope
4. `identity-and-versions` — Identity and versions
5. `kinograph-composition-by-reference` — Kinographs compose by reference
6. `name-vs-id` — Id is authoritative, name is a hint
7. `provenance-mandatory` — Provenance is mandatory
8. `metadata-namespaces` — Metadata namespaces: bare vs `prefix::`
9. `branch-merge-safety` — Branches merge without ledger conflicts
10. `append-only-nothing-mutates` — Append-only: nothing on disk mutates
11. `future-root-kinograph-chains` — Future direction (not yet built)

Composition kinograph: `design-principles`.

**Verification:**
- `kinora resolve design-principles` returns the composed styx listing with all 11 ids resolved.
- `kinora resolve store-content-addressed` returns the markdown body.
- `kinora render --cache-dir /tmp/kinora-dp-site` produces 13 pages (11 + kinograph + pre-existing rfc-0003).
- `mdbook build` on the rendered output succeeds.

**Dogfood observations:**
- Under the hot-ledger layout, every new kino lands in `.kinora/hot/<ab>/<event-hash>.jsonl` — verified all 12 new events are present there.
- Render groups pages under a legacy `7a155b58` branch-label, which is the fallback from the existing RFC-0003 lineage. The render layer's branch-label derivation may deserve a follow-up now that hot events don't belong to any lineage file.
- CLI still prints `lineage=<eh-shorthash> (new lineage)` for every hot write — that language is misleading for hot events. A UX follow-up, not a correctness issue.
