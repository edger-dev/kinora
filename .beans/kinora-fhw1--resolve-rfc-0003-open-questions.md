---
# kinora-fhw1
title: Resolve RFC-0003 open questions
status: completed
type: feature
priority: high
created_at: 2026-04-18T09:16:59Z
updated_at: 2026-04-18T14:23:45Z
parent: kinora-w7w0
---

The 7 open questions from RFC-0003 are resolved through a collaborative brainstorming session. Each decision is recorded below with rationale. Downstream bean bodies have been updated with concrete design references.

## Q1. Hash algorithm

**Decision:** BLAKE3, plain hash mode (unkeyed), 32-byte output encoded as 64-char lowercase hex. Content store path shards by first 2 hex chars: `.kinora/store/aa/aabb…`.

**Rationale:** BLAKE3 has a stable public specification, first-class Rust support, WASM/no-std friendly, and optimized for tree hashing. Since Kinora ships its own CLI, `sha256sum` ubiquity is less critical. Speed is a bonus.

## Q2. Ledger format and file layout

**Decision:**
- **On-disk format:** JSONL — one JSON event per line, append-only
- **Internal serialization:** facet (not serde)
- **File layout:** directory of per-lineage append-only files under `.kinora/ledger/`; no privileged trunk
- **Lineage ID scheme:** filename `<shorthash>.jsonl` where `shorthash` = first 8 hex of BLAKE3 of that lineage's first event
- **Branch bootstrapping:** first `store` on a new git branch mints a new lineage file; `.kinora/HEAD` tracks current lineage

**Rationale:** JSONL is line-oriented (fits append-only), universal (recoverable without special tooling), and handles kinograph list-valued entries naturally. Per-lineage files make git merges structurally conflict-free (all merges are additive file changes). Content-addressed filenames match Kinora's ethos and avoid branch-name coupling.

## Q3. Kinograph format, kino model, and namespacing

**Decision:**
- **Kino model:** identity (immutable BLAKE3 of first content version), version (content hash), metadata (latest-wins per field)
- **Name:** metadata field, not identity; no uniqueness enforcement
- **Version DAG:** each version has `parents[]` that may cross identities — supports linear history, forks, detach (new id, cross-identity parent), combine (new id, multiple cross-identity parents)
- **Kinograph format:** styx with `entries[]`; each entry `{id, name?, pin?, note?}` — id authoritative, name non-authoritative hint, pin optionally freezes to a specific version
- **Data/metadata split:** content (kino) is DAG-tracked with full history; metadata is latest-wins per field (git commit vs git notes analogy)
- **Namespace convention:** bare names = Kinora-reserved; extensions = `prefix::name`; applies to metadata keys, ledger event `kind`, kinograph entry kinds. Tags: convention recommended, not enforced
- **Validation:** strict-on-write (reject unknown bare names), permissive-on-read (preserve unknown namespaced names)
- **Fork resolution:** branch-aware (HEAD's descendant wins) with refuse-and-report fallback; bootstrap = detect-and-report only
- **Variants/detach:** deferred to post-bootstrap; schema accommodates extension

**Rationale:** Names are hard to get right up front. Content-addressed identity matches Kinora's ethos. Cross-identity parents unify detach/combine without new event kinds. Data/metadata line gives clear storage semantics. Namespace convention supports open-closed extension.

## Q4. Concurrent ledger append / conflict handling

**Decision:**
- **Ledger file-level merges:** structurally conflict-free (per-lineage files, Q2)
- **Version DAG forks:** detect at resolve; branch-aware + refuse-and-report; bootstrap = detect-and-report only; manual reconciliation via multi-parent version event
- **Reconciliation event:** regular version event with multiple `parents[]`; no new `kind`
- **Metadata merge:** per-field, ts-latest wins; events carry only changed fields; `null` removes a field
- **Timestamps:** RFC3339 UTC; no clock-skew handling in MVP (document assumption)
- **Write-time validation:** namespace rules + parent/ref existence + kino-id consistency

**Out of MVP scope:** CRDT-style per-element merging (tag set-union etc.), logical clocks, interactive reconcile UX.

## Q5. CLI packaging

**Decision:** Two-crate workspace:
- `kinora` — library crate (store, ledger, kinograph, resolve, render logic)
- `kinora-cli` — binary crate (thin wrapper; binary named `kinora` via `[[bin]] name`)

Library internal structure: modules within `kinora` (`store`, `ledger`, `kinograph`, `resolve`, `render`). Finer-grained splits possible later (mechanical refactor, non-breaking for consumers).

**Rationale:** Clean library boundary from day one; CLI-only deps don't pollute library consumers. Matches existing Cargo workspace. Avoids premature granular splitting.

## Q6. Link format

**Decision:**
- **Two link venues by intent:**
  - **In-content (strong) links:** part of content hash; parsed by kind-specific renderer. Kinograph `entries[]`, markdown `kino://<id>/` URL scheme via native reference-link syntax.
  - **In-metadata (weak) links:** `metadata.links: [{type, id, name?, pin?, note?}]`; queryable, backlinkable
- **`kind` = content type.** Bare kinds = Kinora core: `markdown`, `text`, `binary`, `kinograph`. Extensions namespaced: `kudo::diagram`, `user::sketch`.
- **Markdown link syntax:** standard markdown reference links with `kino://<id>/` URLs — no Kinora-specific syntax invented
- **Backlinks:** derived from ledger scan (metadata links) + content parse (in-content links); indexed in cache layer post-bootstrap
- **MVP kinds:** `markdown` and `kinograph` have parsers/renderers; `text`/`binary` are storeable but opaque in render

**Rationale:** Strong/weak distinction matches natural authoring intent. One `kind` field collapses kind+subtype cleanly. `kino://` URL scheme preserves markdown compatibility.

## Q7. Cache path convention

**Decision:**
- **Cache path:** `~/.cache/kinora/<shorthash>-<name>/`
  - `shorthash` = first 8 hex of `BLAKE3(normalized-repo-url)`
  - `name` = last path segment of normalized URL, sanitized to `[a-z0-9_-]`
- **Config:** `.kinora/config.styx` with required `repo-url` field (only required field); other fields (author defaults, render options) optional
- **URL normalization:** strip scheme, user-info, `.git`, trailing slash; SSH `:` → `/`; lowercase host (path preserved)
- **`kinora init`:** auto-fills `repo-url` from `git remote get-url origin` if available; otherwise errors and prompts user

**Rationale:** Repo URL is the canonical project identity — multiple clones share cache, different projects with same name don't collide. Nix-style `<hash>-<name>` gives readable-but-unambiguous path. Single required config field keeps setup simple.

## Checklist

- [x] Hash algorithm (align with RFC-0001's Moco data model)
- [x] Ledger format (tab-separated, JSON lines, other)
- [x] Kinograph reference format (human-readable and machine-parseable)
- [x] Conflict handling for concurrent ledger appends across branches
- [x] CLI packaging (standalone vs broader Kinora crate)
- [x] Link format (name, hash, both; bidirectional tracking)
- [x] Cache path convention for `<project>` in `~/.cache/kinora/<project>/`

## Summary of Changes

All 7 open questions resolved. Coherent bootstrap design:

- **Hash:** BLAKE3 (64-char hex, sharded `aa/aabb…`)
- **Storage:** content-addressed blobs + per-lineage JSONL ledger files under `.kinora/ledger/`, content-addressed filenames
- **Model:** identity + version DAG (with cross-identity parents) + latest-wins metadata; `kind` names the content type
- **Namespaces:** `prefix::name` for extensions, bare = Kinora-reserved
- **Formats:** markdown kinos with `kino://<id>/` URL scheme; kinograph kinos as styx `entries[]`; binary/text opaque
- **Serialization:** facet internal; JSONL/styx on-disk
- **CLI:** two-crate workspace (`kinora` + `kinora-cli`)
- **Config:** `.kinora/config.styx` with required `repo-url`; cache at `~/.cache/kinora/<shorthash>-<name>/`

Dependent bean bodies (kinora-5k13, kinora-860i, kinora-6zxd, kinora-zboo, kinora-9nom) updated with concrete design references to unblock implementation.
