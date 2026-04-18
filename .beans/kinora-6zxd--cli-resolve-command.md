---
# kinora-6zxd
title: 'CLI: resolve command'
status: todo
type: feature
priority: normal
created_at: 2026-04-18T09:16:59Z
updated_at: 2026-04-18T14:23:45Z
parent: kinora-w7w0
blocked_by:
    - kinora-5k13
---

Given a kino name or id, returns its current content. Supports fork detection and version selection.

RFC-0003 section: *Minimal CLI → resolve*. Design decisions in `kinora-fhw1`.

## Design

### Command shape

```
kinora resolve <name-or-id> [--version HASH] [--all-heads]
```

### Lookup algorithm

1. Scan all lineage files under `.kinora/ledger/`
2. Collect events whose `id` matches (direct lookup) or whose metadata `name` matches (name lookup; warn on ambiguity)
3. Filter to events belonging to this identity (same `id`)
4. Build version DAG: for each event, filter `parents[]` to those of the same identity
5. Find heads (events not referenced as parent by any later same-identity event)
6. Single head → return its content
7. Multiple heads:
   - Branch-aware: if HEAD's lineage descends from one head unambiguously, return that
   - Otherwise: refuse with actionable report

### Fork report shape

```
kino `content-addressing` (id: b3aaa…) has 2 heads:
  - b3xxx… (lineage <shorthash>, yj @ 2026-04-10)
  - b3yyy… (lineage <shorthash>, yj @ 2026-04-12)

Reconcile via one of:
  - merge:     kinora store <kind> --id b3aaa… --parents b3xxx…,b3yyy… <content>
  - linearize: pick one head; write as new version with both as parents
  - keep-both: append metadata event introducing variant names
  - detach:    treat one head as new identity
```

## Acceptance

- [ ] `resolve <id>` returns content of current head
- [ ] `resolve <name>` does name lookup via metadata; warns on ambiguity
- [ ] Fork detection traverses version DAG within identity
- [ ] Single head → return content
- [ ] Multiple heads → refuse with actionable report (heads listed, reconcile commands shown)
- [ ] Branch-aware resolution: if HEAD's lineage descends from one head, prefer it
- [ ] `--version HASH` returns specific prior version's content
- [ ] `--all-heads` flag returns all heads without erroring
- [ ] Unknown name/id yields clear error
