---
# kinora-9nom
title: 'CLI: render command (mdbook)'
status: todo
type: feature
priority: normal
created_at: 2026-04-18T09:16:59Z
updated_at: 2026-04-18T14:23:45Z
parent: kinora-w7w0
blocked_by:
    - kinora-zboo
    - kinora-6zxd
---

Scans local branches and worktrees, resolves kinos and kinographs from each branch's `.kinora/`, produces mdbook output at the cache path.

RFC-0003 sections: *Rendering*, *Minimal CLI → render*. Design decisions in `kinora-fhw1`.

## Design

### Cache path derivation

- Read `.kinora/config.styx → repo-url` (required; error if absent)
- Normalize: strip scheme, user-info, `.git`, trailing `/`; SSH `:` → `/`; lowercase host (path preserved)
- `shorthash` = first 8 hex chars of `BLAKE3(normalized-url)`
- `name` = last path segment of normalized URL, sanitized to `[a-z0-9_-]`
- **Cache path:** `~/.cache/kinora/<shorthash>-<name>/`

### Scanning

1. Enumerate local branches and worktrees
2. For each, read `.kinora/ledger/*.jsonl` visible in that branch's tree
3. Union all events
4. Resolve kinos and kinographs
5. MVP: each branch rendered as a top-level section in SUMMARY.md

### Kind dispatch (MVP)

- `markdown`: render content directly; parse `kino://<id>/` URLs → cross-links to rendered pages
- `kinograph`: concatenate referenced kino contents in order; per-entry notes as blockquotes
- `text`: plain passthrough (fenced code block)
- `binary`: skip with "opaque binary — see source" marker
- Other kinds: skip with warning

### Output layout

```
~/.cache/kinora/<shorthash>-<name>/
  book.toml
  src/
    SUMMARY.md                 # organized by branch
    <branch>/
      <kino-id>.md
      <kinograph-id>.md
```

### Rebuilds

- Full rebuild on every run (no incremental)
- Safe to delete the cache directory; next render regenerates everything

## Acceptance

- [ ] Reads `.kinora/config.styx → repo-url`; errors if absent
- [ ] Derives cache path correctly per normalization rules
- [ ] Renders single current branch end-to-end
- [ ] Extends to all local branches and worktrees (union of ledger files per branch)
- [ ] Kind dispatch: `markdown` + `kinograph` in MVP
- [ ] `kino://<id>/` URLs resolved to cross-links between rendered pages
- [ ] SUMMARY.md organized by branch
- [ ] Source markers include originating branch
- [ ] Full rebuild on every run
- [ ] Output is viewable via `mdbook serve` from cache path
