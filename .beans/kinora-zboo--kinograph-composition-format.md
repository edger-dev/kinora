---
# kinora-zboo
title: Kinograph composition format
status: todo
type: feature
priority: normal
created_at: 2026-04-18T09:16:59Z
updated_at: 2026-04-18T14:23:45Z
parent: kinora-w7w0
blocked_by:
    - kinora-5k13
---

Kinograph composition — `kind: kinograph` with styx `entries[]` content. Each entry references a kino by identity.

RFC-0003 section: *Kinographs*. Design decisions in `kinora-fhw1`.

## Design

### Content format (styx)

```styx
entries:
  - id: b3aaa…
  - id: b3bbb…
    name: content-addressing
    pin: b3xxx…
    note: "The atomic concept — everything else builds on this."
  - id: b3ccc…
```

- `id` (required): authoritative kino-id reference
- `name` (optional): non-authoritative hint; renderer warns if current name differs
- `pin` (optional): freeze this reference to a specific content hash
- `note` (optional): short commentary about this composition choice

### Metadata on ledger event

- `title` — human title
- `description` — longer prose describing the composition
- `entry_notes` — optional per-entry notes keyed by kino id
- Namespaced extensions allowed

### Authoring flow

1. User writes kinograph source with names or ids
2. `kinora store kinograph <path>` resolves names to ids against current ledger state
3. Stored content has ids filled in (authoritative); name hints preserved
4. Raw-file readability preserved
5. Renaming a referenced kino later does not break the kinograph (id is stable)

### Rendering

- Walk `entries[]` in order
- For each entry: resolve `id` (or pinned hash); fetch kino content; inline
- If referenced kino is itself a kinograph: recurse (stretch goal) or warn
- Optional per-entry notes rendered as leading blockquote

## Acceptance

- [ ] Parses styx kinograph with `entries[]`
- [ ] Entry shape validated: `{id, name?, pin?, note?}`
- [ ] Name→id resolution on store (warn on ambiguous or missing)
- [ ] Pinned refs resolve to specific content hash
- [ ] Raw file remains human-readable for emergency recovery
- [ ] Updates append new ledger events (version DAG preserved)
- [ ] Renderer concatenates resolved entries in order
- [ ] Per-entry notes emitted as blockquote above entry content
