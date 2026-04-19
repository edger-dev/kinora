---
# kinora-xi21
title: 'Architecture: hot ledger + root kinograph chains'
status: draft
type: epic
priority: normal
created_at: 2026-04-19T05:38:37Z
updated_at: 2026-04-19T05:38:37Z
---

Post-bootstrap architecture for kinora's history and compaction model. Supersedes the current ledger-per-kino layout (`.kinora/ledger/<lineage>.jsonl`) and retires earlier sketches of a separate "cold ledger" format.

The core insight: kinora self-hosts its own history using the kinograph primitive. Chained root kinographs act as canonical snapshots (analogous to git commits); a one-file-per-event hot ledger is the mempool for unpromoted writes; compaction on `main` is the commit discipline.

## Four-concept model

- **Kino** — identity (birth hash) + content versions. Never carries bookkeeping metadata about which root it belongs to.
- **Root** — a named, `root`-kinded kinograph whose versioned lineage enumerates kinos under one GC/prune policy. May be Merkle-structured.
- **Assign** — an explicit hot-ledger operation recording "kino X should live in root Y from now on." Consumed by compaction.
- **Compaction** — the operation (run only on the `main` branch) that promotes hot events into new versions of the affected root kinographs and prunes aged events per each root's policy.

## File layout

```
.kinora/
  config.styx                     # declares named roots + policies
  hot/<ab>/<event-hash>.jsonl     # one event per file; immutable; merge-safe
  store/<ab>/<hash>.<ext>         # global content-addressed blobs
  roots/<name>                    # pointer file: current head version of each root
```

## Key design decisions

### 1. Hot ledger is one-event-per-file, sharded by event hash

Each store call appends a single-file event record. Merges of branches produce set-union of files → zero JSONL conflicts, no branch-name coupling. Readers union hot events and dedup by event hash.

### 2. Canonical history = chain of root kinograph versions

A root kinograph's entries enumerate its kinos (directly, or via sub-kinographs). Each version has `parents[]` → previous version(s) of the same root's lineage. Structurally parallels git: root kinograph ≈ commit, kino blob ≈ blob, sub-kinograph ≈ tree.

### 3. `root` is a reserved kind

Joins `markdown`, `text`, `binary`, `kinograph` as a core kind (not namespaced). Enables type-level invariant enforcement and specialized tooling.

### 4. Main-only compaction

Only commits on the branch named by `config.styx → main-branch` (default `main`) trigger compaction. Other branches accumulate hot events as speculative proposals; render shows branch-only events as temp versions vs main's sealed versions. Mechanism: git post-commit / post-merge hook.

### 5. Multiple named roots with per-root policy

Repos declare any number of named roots in config. Each has its own GC/prune policy (e.g. `never`, `30d`, `keep-last-N`). Motivates topical partitioning.

### 6. Inbox is the default root

Unassigned kinos land in `inbox`. Default policy is aggressive (e.g. `30d`), to nudge triage discipline. Auto-provisioned if not declared.

### 7. Metadata ownership is structural, not declared

A kino's metadata home is its leaf position in its owning root's Merkle tree. Entries inside a root tree inline full metadata (`name`, `title`, etc.). Kinographs **outside** any root tree (user-created composition kinographs) carry pure `{id, version}` pointers. Readers resolve "current metadata for kino X" via: find X's owning root → read inline metadata at its leaf.

Composition is preserved — a kino can appear in many kinographs; only ownership is exclusive.

### 8. Kinos never declare root membership

No `root=X` metadata on kinos. Identity is the sole stable property. Relationship flows owner→owned: roots claim kinos, not the other way around. Moving between roots is a root-level operation (assign event → compaction).

### 9. Cross-root references and pinning

Cross-root `kino://` references are normal composition. GC could drop older versions of referenced kinos; pins prevent this. Pin = explicit "keep this version regardless of policy."

## Phased delivery

Each phase is an independent follow-on bean.

1. **Hot ledger: one-file-per-event** — retire `ledger/<lineage>.jsonl`, switch to `hot/<ab>/<event-hash>.jsonl`. Independent of the root model; prerequisite for everything else.
2. **`root` kind + single flat root + `kinora compact`** — define the `root` kind, implement compaction on main producing one flat root kinograph version, wire `roots/<name>` pointer files.
3. **Multiple named roots + `assign` event + per-root GC** — config-declared roots, `inbox` default, assign/compact/prune workflow. Requires generalizing the hot-ledger event schema beyond store events.
4. **Merkle sub-kinographs inside roots** — enable O(log N) diff semantics for large repos.

## Scope boundaries

- **In scope:** data model, file layout, compaction model, GC/prune semantics, metadata ownership.
- **Deferred:** branch enumeration & multi-branch render (kinora-ohwb continues), cross-repo federation, remote fetch, UI rendering.

## Open questions (not blocking; refine during phase beans)

- Canonical styx schema for root kinograph entries
- Git hook shape: pre-commit, post-commit, post-merge, or wrapper command?
- Fast-forward merges and rebases: how do we ensure compaction is idempotent?
- Genesis root: any special form, or identical except for no parents?
- Merkle grouping rule for sub-kinographs: hash-prefix (automatic) or user-declared topic (curated)?

## Supersedes

- Current: `.kinora/ledger/<lineage>.jsonl` per-kino ledger files
- Earlier sketch: separate "cold ledger" compacted JSONL format (rejected in favor of self-hosted root kinographs)

## Related

- `kinora-wpup` — store filename extension — compatible, no conflict
- `kinora-cium` — dogfood RFC import — unaffected; imported kinos will migrate to `rfcs` root once roots land
- `kinora-ohwb` — multi-branch render enumeration — will integrate with hot/sealed view

## Provenance

Design converged through a joint YJ + Claude session on 2026-04-19. This bean captures the shape but is not itself implementation-ready — phase beans will carry the actual work.
