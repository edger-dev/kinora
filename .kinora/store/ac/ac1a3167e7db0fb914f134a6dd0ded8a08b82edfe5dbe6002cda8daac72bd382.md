# Plan — a root's versions are an append-only stream

**Task:** kino://a1bac9ab38a30da7fd5d5dde31176490b567a3a9625d4eb119649175bac7e9b6

Each phase: tests → implementation → fresh review → fixes.

1. **Roots on a stream.** `kinora` depends on `moco-place` (git rev); `Hash`
   re-exported from it. A `roots` module: root names (stream directories and
   legacy pointer files), a root's heads (a stream's heads through each
   version's header parents; a legacy file's one hash), append a version,
   migrate a legacy pointer (renamed aside, streamed, removed — read from the
   aside copy if a crash interrupts).
2. **Commit folds; readers read every head.** Commit migrates legacy
   pointers, then builds from every head: prior entries unioned, a kino the
   fresh build does not cover taken from whichever head descends, a genuine
   fork refused naming the kinos to merge; the new version names every head
   as a parent, written even when its entries are unchanged. resolve,
   render, clone, repack read heads through the module.
3. **Docs and specs.** README/CLAUDE guidance: commit is no longer
   main-only. Re-version kinora's mirror spec and content-addressing
   extraction spec (their home is moco now).
4. **Safe fixes (author's go-ahead, 25 Oct).** Event files hold exactly
   their line, written through the place. A fold over a history with a
   version missing from the store refuses, naming it. The history walk is
   linear: headers only for the walk, the base found in one pass. An
   assign-only move of a committed kino carries its entry, from the batch's
   ownership snapshot, into the target root.

## Status

- Phase 1 done (bc2aca4, 5c1ac56, review 21e2a6b): the roots module on moco's
  stream; the ledger is a place *within* the repository (moco 4117c76), so
  reads write nothing; git's conflict copy `roots/<name>~<branch>` and an
  aside copy count as heads until migrated; root names are stream names.
- Phase 2 done (6673fa7, 45a5683; reviews 088cebc, a7c71f0, 0446e28): the
  fold is three-way against the heads' common base (`RootHistory`): per kino
  a head left, changed, dropped or added it; a head that left it needs no
  proof; dropped against changed, conflicting pins, and versions none of
  which descends from the rest are disputes, refused until merged. A
  fold-only commit is stamped from its heads, so two machines agree.
  resolve reads heads through the same fold. The content store writes
  through the ledger's place (376826c).
- Phase 4 done (d29a79e, ae68116; review d2715cf, 9a6ff8a): event files hold
  exactly their line, through the place (kinora-remote takes both forms); the
  walk reads header lines once and finds the base in one pass (600 versions
  well under 2 s), refusing when a version not older than the base is
  missing; a clone carries a forked root's history (a gap: heads only,
  reported); an assign-only move carries the entry from the batch snapshot,
  refusing when the entry cannot be taken; the root that commits a move owns
  the assigns it superseded.
- **Open, for the author:** criss-cross merges (several latest common
  ancestors; the base picked by hash decides whether a moved kino returns);
  a fork settled together with a move wedges the old root; a fold across a
  MaxAge cutoff on two clocks; committing one root alone (not `kinora
  commit`, which commits all) can leave a moved kino in both roots;
  superseded assigns the old binary already stranded still read as live;
  the walk still reaches genesis and runs twice per forked root per commit.
- **Rollout notes:** an old binary's kinora-remote pull rejects event files
  without the newline, so mixed versions sharing a mirror stop syncing
  events — update every machine together (already required by streams).
- Phase 3: the two specs re-versioned (mirror: superseded in direction;
  extraction: home is moco). **Held for the author:** the kinora jig bump and
  the CLAUDE template's "commit only on main" text must land together — the
  first commit by a new binary migrates a repo's roots, which older binaries
  then cannot read, so every machine updates before anyone commits.
