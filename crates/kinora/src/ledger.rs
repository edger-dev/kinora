use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::event::{Event, EventError};
use crate::hash::Hash;
use crate::paths::{staged_dir, staged_event_path, STAGED_EXT};

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger io error: {0}")]
    Io(#[from] io::Error),
    #[error("ledger event error: {0}")]
    Event(#[from] EventError),
}

pub struct Ledger {
    kinora_root: PathBuf,
}

impl Ledger {
    pub fn new(kinora_root: impl Into<PathBuf>) -> Self {
        Self { kinora_root: kinora_root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.kinora_root
    }

    pub fn ensure_layout(&self) -> Result<(), LedgerError> {
        fs::create_dir_all(staged_dir(&self.kinora_root))?;
        Ok(())
    }

    /// Write `event` to `.kinora/staged/<ab>/<event-hash>.jsonl`. Crash-atomic
    /// via tmp+rename: a crash mid-write leaves an orphan tmp but never a
    /// truncated target, so a follow-up call always sees either a complete
    /// file or no file at all. Idempotent: if the target already exists
    /// (event hash is content-addressed, so identical path implies identical
    /// content), returns the hash without rewriting.
    ///
    /// Returns `(event_hash, was_new)` — `was_new` is true iff the target
    /// file did not exist before this call.
    pub fn write_event(&self, event: &Event) -> Result<(Hash, bool), LedgerError> {
        self.ensure_layout()?;
        let line = event.to_json_line()?;
        let event_hash = Hash::of_content(line.as_bytes());
        let path = staged_event_path(&self.kinora_root, &event_hash);
        if path.is_file() {
            return Ok((event_hash, false));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension(format!("{STAGED_EXT}.tmp"));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(line.as_bytes())?;
            f.write_all(b"\n")?;
        }
        fs::rename(&tmp, &path)?;
        Ok((event_hash, true))
    }

    /// Return every event stored under `.kinora/staged/`, deduped by event hash.
    #[fastrace::trace]
    pub fn read_all_events(&self) -> Result<Vec<Event>, LedgerError> {
        let dir = staged_dir(&self.kinora_root);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<Event> = Vec::new();
        for shard in fs::read_dir(&dir)? {
            let shard = shard?;
            if !shard.file_type()?.is_dir() {
                continue;
            }
            for entry in fs::read_dir(shard.path())? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some(STAGED_EXT) {
                    continue;
                }
                for event in read_events(&path)? {
                    let eh = event.event_hash()?;
                    if seen.insert(eh.as_hex().to_owned()) {
                        out.push(event);
                    }
                }
            }
        }
        Ok(out)
    }
}

fn read_events(path: &Path) -> Result<Vec<Event>, LedgerError> {
    let f = fs::File::open(path)?;
    let r = BufReader::new(f);
    let mut out = Vec::new();
    for line in r.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        out.push(Event::from_json_line(&line)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn ledger() -> (TempDir, Ledger) {
        let tmp = TempDir::new().unwrap();
        let kin = tmp.path().to_path_buf();
        let l = Ledger::new(&kin);
        l.ensure_layout().unwrap();
        (tmp, l)
    }

    fn event(n: &str, ts: &str) -> Event {
        let h = Hash::of_content(n.as_bytes());
        Event::new_store(
            "markdown".into(),
            h.as_hex().into(),
            h.as_hex().into(),
            vec![],
            ts.into(),
            "yj".into(),
            "test".into(),
            BTreeMap::from([("name".to_string(), n.to_string())]),
        )
    }

    #[test]
    fn write_event_creates_sharded_file_at_event_hash_path() {
        let (_t, l) = ledger();
        let e = event("staged-first", "2026-04-19T09:00:00Z");
        let expected = e.event_hash().unwrap();
        let (returned, was_new) = l.write_event(&e).unwrap();
        assert_eq!(returned, expected);
        assert!(was_new);
        let path = staged_event_path(l.root(), &expected);
        assert!(path.is_file(), "staged event file missing: {}", path.display());
        // Sharded by the first two hex chars.
        assert!(path.to_string_lossy().contains(&format!("/staged/{}/", expected.shard())));
    }

    #[test]
    fn write_event_file_contains_exactly_one_line() {
        let (_t, l) = ledger();
        let e = event("staged-one-line", "2026-04-19T09:00:00Z");
        let (h, _) = l.write_event(&e).unwrap();
        let contents = fs::read_to_string(staged_event_path(l.root(), &h)).unwrap();
        assert!(contents.ends_with('\n'), "file should end with newline: {contents:?}");
        assert_eq!(contents.matches('\n').count(), 1, "expected one line: {contents:?}");
    }

    #[test]
    fn write_event_is_idempotent_when_same_event_stored_twice() {
        let (_t, l) = ledger();
        let e = event("dup", "2026-04-19T09:00:00Z");
        let (h1, new1) = l.write_event(&e).unwrap();
        let (h2, new2) = l.write_event(&e).unwrap();
        assert_eq!(h1, h2);
        assert!(new1);
        assert!(!new2, "second write should report the file was not new");
    }

    #[test]
    fn read_all_events_returns_empty_when_no_staged_dir() {
        let (_t, l) = ledger();
        // ensure_layout() has been called in `ledger()`, but the staged dir may be
        // empty — verify this gives us an empty list, not an error.
        let got = l.read_all_events().unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn read_all_events_roundtrips_written_event() {
        let (_t, l) = ledger();
        let e = event("rt", "2026-04-19T09:00:00Z");
        l.write_event(&e).unwrap();
        let got = l.read_all_events().unwrap();
        assert_eq!(got, vec![e]);
    }

    #[test]
    fn read_all_events_dedups_across_multiple_writes() {
        let (_t, l) = ledger();
        let e = event("same", "2026-04-19T09:00:00Z");
        l.write_event(&e).unwrap();
        l.write_event(&e).unwrap();
        let got = l.read_all_events().unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn read_all_events_returns_every_distinct_event() {
        let (_t, l) = ledger();
        let a = event("a", "2026-04-19T09:00:00Z");
        let b = event("b", "2026-04-19T09:00:01Z");
        let c = event("c", "2026-04-19T09:00:02Z");
        l.write_event(&a).unwrap();
        l.write_event(&b).unwrap();
        l.write_event(&c).unwrap();
        let mut got = l.read_all_events().unwrap();
        got.sort_by(|x, y| x.ts.cmp(&y.ts));
        assert_eq!(got, vec![a, b, c]);
    }

    #[test]
    fn read_all_events_ignores_non_jsonl_files_in_shard_dirs() {
        let (_t, l) = ledger();
        let e = event("x", "2026-04-19T09:00:00Z");
        let (h, _) = l.write_event(&e).unwrap();
        let shard = staged_dir(l.root()).join(h.shard());
        fs::write(shard.join("junk.txt"), b"ignore me").unwrap();
        let got = l.read_all_events().unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn cross_branch_merge_simulation_unions_files_without_conflict() {
        // Simulate two branches independently writing events. A set-union
        // of their staged dirs (as git merge would produce) should yield the
        // union of events with no overlap issues.
        let base = TempDir::new().unwrap();
        let a_root = base.path().join("a");
        let b_root = base.path().join("b");
        let merged = base.path().join("merged");

        let la = Ledger::new(&a_root);
        la.ensure_layout().unwrap();
        let lb = Ledger::new(&b_root);
        lb.ensure_layout().unwrap();

        let ea = event("branch-a", "2026-04-19T09:00:00Z");
        let eb = event("branch-b", "2026-04-19T09:00:01Z");
        la.write_event(&ea).unwrap();
        lb.write_event(&eb).unwrap();

        // "Merge" = copy both staged trees into the merged location.
        let lm = Ledger::new(&merged);
        lm.ensure_layout().unwrap();
        for src in [&a_root, &b_root] {
            copy_staged_tree(&staged_dir(src), &staged_dir(&merged));
        }

        let mut got = lm.read_all_events().unwrap();
        got.sort_by(|x, y| x.ts.cmp(&y.ts));
        assert_eq!(got, vec![ea, eb]);
    }

    #[test]
    fn write_event_leaves_no_tmp_file_on_success() {
        let (_t, l) = ledger();
        let e = event("tmp-cleanup", "2026-04-19T09:00:00Z");
        let (h, _) = l.write_event(&e).unwrap();
        let shard = staged_dir(l.root()).join(h.shard());
        let tmp = shard.join(format!("{}.jsonl.tmp", h.as_hex()));
        assert!(!tmp.exists(), "orphan tmp left behind: {}", tmp.display());
    }

    #[test]
    fn write_event_recovers_from_orphan_tmp_from_a_prior_crash() {
        // Simulate a crash during a previous write that left a tmp file
        // alongside no real event file. The next write should still succeed.
        let (_t, l) = ledger();
        let e = event("recover", "2026-04-19T09:00:00Z");
        let event_hash = e.event_hash().unwrap();
        let path = staged_event_path(l.root(), &event_hash);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let tmp = path.with_extension("jsonl.tmp");
        fs::write(&tmp, b"garbage from a crash").unwrap();

        let (h, was_new) = l.write_event(&e).unwrap();
        assert_eq!(h, event_hash);
        assert!(was_new);
        assert!(path.is_file());
    }

    #[test]
    fn write_event_file_name_is_event_hash_with_jsonl_ext() {
        let (_t, l) = ledger();
        let e = event("filename-shape", "2026-04-19T09:00:00Z");
        let (h, _) = l.write_event(&e).unwrap();
        let path = staged_event_path(l.root(), &h);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name, format!("{}.jsonl", h.as_hex()));
    }

    fn copy_staged_tree(src: &Path, dst: &Path) {
        if !src.exists() {
            return;
        }
        fs::create_dir_all(dst).unwrap();
        for shard in fs::read_dir(src).unwrap() {
            let shard = shard.unwrap();
            if !shard.file_type().unwrap().is_dir() {
                continue;
            }
            let shard_dst = dst.join(shard.file_name());
            fs::create_dir_all(&shard_dst).unwrap();
            for entry in fs::read_dir(shard.path()).unwrap() {
                let entry = entry.unwrap();
                let from = entry.path();
                let to = shard_dst.join(entry.file_name());
                fs::copy(&from, &to).unwrap();
            }
        }
    }
}
