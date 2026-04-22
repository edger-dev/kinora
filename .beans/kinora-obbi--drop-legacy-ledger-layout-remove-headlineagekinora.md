---
# kinora-obbi
title: 'Drop legacy ledger layout: remove HEAD/lineage/.kinora/ledger code'
status: completed
type: task
priority: normal
created_at: 2026-04-22T13:58:11Z
updated_at: 2026-04-22T14:06:15Z
---

Post-mjvb + et1t, .kinora/ledger/ is vestigial — no production write path touches it, and resolve only reads it as a backward-compat migration stub. Kill the legacy code paths entirely so the dir stops getting mkdir'd and the resolver code shrinks.

## Scope (removals)

- `init.rs`: drop `fs::create_dir_all(ledger_dir(&root))` + the `ledger_dir` import. Drop init-test assertions for `ledger_dir` / `head_path`.
- `clone.rs`: drop `fs::create_dir_all(ledger_dir(dst))` + the `ledger_dir` import. Drop clone-test assertion for `ledger_dir`.
- `ledger.rs`: remove legacy APIs and their tests — `mint_and_append`, `append_to_head`, `read_lineage`, `read_all_lineages`, `current_lineage`, `set_head`, `LedgerError::{NoHead, LineageAlreadyExists, LineageMissing}`, and the `ledger_dir` mkdir in `ensure_layout`.
- `resolve.rs`: remove the `read_all_lineages` loop in `Resolver::load`; remove `head_for_current_lineage` and its callsite (multi-head → always `MultipleHeads`); remove the 5 legacy-format resolver tests that mint via `mint_and_append`.
- `paths.rs`: remove `head_path`, `ledger_dir`, `ledger_file_path`, `HEAD_FILE`, `LEDGER_DIR`, `LEDGER_EXT` + their tests.

## Acceptance

- [x] All tests pass; zero warnings.
- [x] `rg ledger_dir|head_path|HEAD_FILE|LEDGER_` returns no matches in crates/kinora/src.
- [x] Fresh `init` does NOT create `.kinora/ledger` or `.kinora/HEAD`.

## Hard cutover

Pre-mjvb repos (if any still exist) will lose resolution of legacy lineage-based events. Matches the et1t hard-cutover spirit. Current dogfood repo already rebuilt post-et1t so no local impact.

## Summary of Changes

Removed all pre-mjvb ledger code:

- **paths.rs**: dropped `HEAD_FILE`, `LEDGER_DIR`, `LEDGER_EXT` constants and `head_path`, `ledger_dir`, `ledger_file_path` functions + their tests.
- **init.rs** & **clone.rs**: dropped `ledger_dir` mkdir; tests now assert `.kinora/ledger` and `.kinora/HEAD` are NOT created.
- **ledger.rs**: removed `mint_and_append`, `append_to_head`, `read_lineage`, `read_all_lineages`, `current_lineage`, `set_head` methods; removed `NoHead`/`LineageAlreadyExists`/`LineageMissing` error variants; removed helpers `write_line_exclusive`/`append_line_existing`; removed ~11 legacy tests. `ensure_layout` now only creates `staged/`.
- **resolve.rs**: dropped the `read_all_lineages` loop in `Resolver::load`; removed `head_for_current_lineage` method and its callsite in `pick_head` (multi-head always returns `MultipleHeads`); removed 3 legacy-format tests.
- **kinora-cli/src/resolve.rs**: removed two `remove_file(head_path(...))` calls in CLI fork tests that defended against the now-gone legacy tiebreak.

Hard cutover: pre-mjvb repos lose resolution of legacy lineage-based events. Matches the et1t cutover spirit. Local dogfood repo was rebuilt post-et1t so no impact.
