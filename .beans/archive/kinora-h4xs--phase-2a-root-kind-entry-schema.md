---
# kinora-h4xs
title: 'Phase 2A: `root` kind + entry schema'
status: completed
type: task
priority: normal
created_at: 2026-04-19T06:50:32Z
updated_at: 2026-04-19T07:01:04Z
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

- [x] `root` added to reserved kinds; all namespace tests updated
- [x] `ext_for_kind("root") == Some("styx")` + test
- [x] Root entries parse via dedicated `RootKinograph` module (see spec departure in Summary)
- [x] Duplicate-id within one root → parse error (test)
- [x] Parser validation: invalid hash / bad kind / bad metadata key all rejected (tests)
- [x] Roundtrip: parse → `to_styx()` → parse produces identical AST; output is canonical (entries sorted by id, metadata keys sorted) (test)
- [x] All tests pass; zero compiler warnings (212 + 41 in workspace)

## Out of scope

- Compaction logic (phase 2B)
- `roots/<name>` pointer files (phase 2B)
- Sub-kinograph entries (phase 4)
- Multi-root / config-declared roots / GC (phase 3)

## Summary of Changes

Landed in three commits:

1. **`root: introduce RootKinograph module + tests (kinora-h4xs)`** (ae86151) — new `crates/kinora/src/root.rs` with `RootEntry` / `RootKinograph` types, parse/serialize via facet-styx, canonical sort-by-id, duplicate-id rejection, hash/kind/metadata-key validation. 15 tests (13 green on commit, 2 red pending namespace flip).
2. **`namespace: reserve root kind + styx extension (kinora-h4xs)`** (a491b45) — adds `"root"` to `RESERVED_KINDS` and `ext_for_kind("root") -> Some("styx")`. Flips the 2 reds green.
3. **`root: review fixes — drop dead variant, add missing tests (kinora-h4xs)`** (a0abeea) — dropped unreachable `RootError::Utf8` + its unused `From` impl; added 4 tests pinning the metadata-key sort order, `parse(bytes)` path, invalid-UTF-8 behavior, and the `#[facet(default)]` contract for `pin`/`note`.

### Spec departure

The bean spec proposed extending `Kinograph`/`Entry` in `crates/kinora/src/kinograph.rs` with a second entry variant. I shipped a **separate `RootKinograph` module instead**. Reasons:

- Composition and root entries have meaningfully different semantics for the `pin` field: composition `pin` is a 64-hex version hash, root `pin` is a boolean GC-exemption marker. Overloading the same field name on one struct would be confusing and error-prone.
- facet-styx doesnt have a native tagged-enum form that would let a single struct gracefully carry both variants; a single-struct-with-all-fields approach would leak root-only fields (`version`, `kind`, `metadata`) into composition entries and vice versa.
- Separate modules keep each focused. Callers already know which kind (`kinograph` vs `root`) theyre parsing because the ledger event carries it — no dispatch needed.

The specs "Parser selects variant by presence of `kind`" auto-detect idea was rejected in favor of explicit module selection. Root and composition kinographs share the styx format but not the entry shape; treating them as distinct types at the rust-level matches that.

### Facet-styx capabilities proven

Incidental finding: facet-styx handles `bool` and `BTreeMap<String, String>` nested inside a `Vec<struct>` cleanly — no sentinel-string workaround needed for the `pin` boolean. Useful to know for phase 2B and phase 3 schema work.
