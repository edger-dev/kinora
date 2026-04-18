---
# kinora-860i
title: 'CLI: store command'
status: todo
type: feature
priority: normal
created_at: 2026-04-18T09:16:59Z
updated_at: 2026-04-18T14:23:45Z
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

- [ ] Accepts content from stdin and file path
- [ ] Records name (in metadata), kind, provenance, draft flag, timestamp
- [ ] Provenance required (errors if missing)
- [ ] Dedups store writes by hash; still appends ledger event
- [ ] Validates namespace rules (reject unknown bare metadata keys)
- [ ] Validates parent hashes exist in store
- [ ] Mints new lineage file on first store in new branch
- [ ] Appends to current lineage file when lineage exists
- [ ] Updates `.kinora/HEAD` when minting new lineage
- [ ] After running, workspace ready for `git add` + `git commit`
