---
# kinora-w7w0
title: Kinora bootstrap
status: todo
type: epic
priority: normal
created_at: 2026-04-18T09:16:59Z
updated_at: 2026-04-19T05:38:46Z
---

## Scope

Implement a minimal file-based Kinora usable via CLI, per RFC-0003 at `~/edger/kudo/docs/src/rfcs/rfc-0003_kinora-bootstrap.md`.

## Features (dependency order)

1. Resolve RFC-0003 open questions (decisions gate)
2. Data layer: content store + append-only ledger
3. CLI: `store` command
4. CLI: `resolve` command
5. Kinograph composition format
6. CLI: `render` command (mdbook output)
7. Dogfood: migrate kudo RFCs as first kinos

## Out of scope

Beans-sync (kinograph → beans task spec) is deferred to post-bootstrap.

## Done when

All child features complete; dogfood surfaces no blocking issues.

## Post-bootstrap direction

Canonical history model (hot ledger + chained root-kinograph snapshots) is captured in `kinora-xi21`. Once the bootstrap CLI (store/resolve/render) is solid, the architecture evolves along that path: one-file-per-event hot ledger, `root` kind, compaction on main, multiple named roots with `inbox` as default.

The current ledger-per-kino layout in the bootstrap is explicitly transitional — `kinora-xi21` supersedes it.
