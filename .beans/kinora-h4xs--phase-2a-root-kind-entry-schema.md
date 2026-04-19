---
# kinora-h4xs
title: 'Phase 2A: `root` kind + entry schema'
status: in-progress
type: task
priority: normal
created_at: 2026-04-19T06:50:32Z
updated_at: 2026-04-19T06:51:05Z
parent: kinora-xi21
---

Phase 2A of the kinora-xi21 architecture. Adds the `root` reserved kind and extends the kinograph format to accept flat root entries (one entry per owned kino, with inline metadata).

Prereq: kinora-mjvb (hot ledger, done), kinora-wpup (store extension, done).

## Scope

### Reserved kind

Add `root` to the reserved-kind list in `crates/kinora/src/namespace.rs`:

- `markdown`, `text`, `binary`, `kinograph` exist today — `root` joins them
- `ext_for_kind("root") -> Some("styx")` (same extension as kinograph — roots are kinographs)

### Root entry schema (flat form)

Extend the kinograph parser in `crates/kinora/src/kinograph.rs` to accept a second entry variant:

```
entry {
  id <kino-id>                     # required
  version <content-hash>           # required; pins a specific version
  kind <kind>                      # required; kino's kind
  metadata { name …, title …, … } # required; bare-key and prefix:: rules apply
  note? <text>                     # optional composition hint
  pin? true                        # optional; exempt from GC
}
```

The existing composition entry (pure `{id, version?, note?}` pointers) stays in place — roots use the richer leaf form, composition kinographs use the slim form. Parser selects variant by presence of `kind` (or any metadata block).

### Validation rules

- `id` must parse as a valid 64-hex hash
- `version` must parse as a valid 64-hex hash
- `kind` must pass `namespace::validate_kind` (reserved or `prefix::`)
- Metadata keys must pass `namespace::validate_metadata_key`
- Duplicate `id` entries in one root → parse error (a root must own each kino exactly once)

### Serialization

`to_styx()` must emit entries sorted by `id` (ascii-hex order). Metadata keys sorted within each entry. Canonical form is the only form — no "style" options.

## Acceptance

- [ ] `root` added to reserved kinds; all namespace tests updated
- [ ] `ext_for_kind("root") == Some("styx")` + test
- [ ] Kinograph parser accepts flat root entries; existing composition entries still parse
- [ ] Duplicate-id within one root → parse error (test)
- [ ] Parser validation: invalid hash / bad kind / bad metadata key all rejected (tests)
- [ ] Roundtrip: parse → `to_styx()` → parse produces identical AST; output is canonical (entries sorted by id, metadata keys sorted) (test)
- [ ] All tests pass; zero compiler warnings

## Out of scope

- Compaction logic (phase 2B)
- `roots/<name>` pointer files (phase 2B)
- Sub-kinograph entries (phase 4)
- Multi-root / config-declared roots / GC (phase 3)
