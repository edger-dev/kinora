//! Read `.kinora/` state from specific git commits.
//!
//! Multi-branch render (kinora-ohwb) walks every local branch and worktree
//! tip, extracting each `.kinora/` subtree into a scratch directory so the
//! existing file-based `Resolver::load` machinery can read it without
//! caring that the bytes came from a git tree object. This module owns
//! that gix-side plumbing: branch enumeration, worktree enumeration, and
//! the tree-walk that materializes blobs onto disk.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::objs::tree::EntryKind;

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("git-state io error at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("git object lookup failed: {0}")]
    Lookup(String),
    #[error("git object decode failed: {0}")]
    Decode(String),
    #[error("subtree `{path}` not present in commit {commit}")]
    SubtreeAbsent { commit: ObjectId, path: String },
}

#[derive(Debug, thiserror::Error)]
pub enum EnumError {
    #[error("reference enumeration failed: {0}")]
    References(String),
    #[error("failed to peel reference `{name}`: {cause}")]
    Peel { name: String, cause: String },
    #[error("worktree enumeration failed: {0}")]
    Worktrees(String),
}

/// Extract the contents of `<subtree_path>` from `commit_oid`'s tree into
/// `dst`. The root of `dst` corresponds to the contents *inside* the
/// subtree (e.g. extracting `.kinora` writes `config.styx` directly under
/// `dst`, not under `dst/.kinora/`).
///
/// Blobs and executable blobs are written; symlinks and gitlinks are
/// silently skipped — neither can belong to a content-addressed kinora
/// store, and following them would introduce host-dependent I/O.
pub fn extract_subtree(
    repo: &gix::Repository,
    commit_oid: ObjectId,
    subtree_path: &str,
    dst: &Path,
) -> Result<(), ExtractError> {
    let commit = repo
        .find_object(commit_oid)
        .map_err(|e| ExtractError::Lookup(format!("commit {commit_oid}: {e}")))?
        .try_into_commit()
        .map_err(|e| ExtractError::Decode(format!("commit {commit_oid}: {e}")))?;

    let root_tree_id = commit
        .tree_id()
        .map_err(|e| ExtractError::Decode(format!("commit {commit_oid} tree_id: {e}")))?
        .detach();
    let root_tree = repo
        .find_object(root_tree_id)
        .map_err(|e| ExtractError::Lookup(format!("tree {root_tree_id}: {e}")))?
        .try_into_tree()
        .map_err(|e| ExtractError::Decode(format!("tree {root_tree_id}: {e}")))?;

    // Descend subtree_path one component at a time. If any component is
    // missing or is a blob instead of a tree, surface SubtreeAbsent.
    let mut current_tree = root_tree;
    for component in subtree_path.split('/').filter(|c| !c.is_empty()) {
        let entry = current_tree
            .iter()
            .filter_map(Result::ok)
            .find(|e| e.filename().to_str().ok() == Some(component));
        let Some(entry) = entry else {
            return Err(ExtractError::SubtreeAbsent {
                commit: commit_oid,
                path: subtree_path.to_owned(),
            });
        };
        if entry.mode().kind() != EntryKind::Tree {
            return Err(ExtractError::SubtreeAbsent {
                commit: commit_oid,
                path: subtree_path.to_owned(),
            });
        }
        let oid = entry.object_id();
        current_tree = repo
            .find_object(oid)
            .map_err(|e| ExtractError::Lookup(format!("tree {oid}: {e}")))?
            .try_into_tree()
            .map_err(|e| ExtractError::Decode(format!("tree {oid}: {e}")))?;
    }

    fs::create_dir_all(dst).map_err(|source| ExtractError::Io {
        path: dst.to_path_buf(),
        source,
    })?;
    write_tree_recursive(repo, &current_tree, dst)
}

fn write_tree_recursive(
    repo: &gix::Repository,
    tree: &gix::Tree,
    dst: &Path,
) -> Result<(), ExtractError> {
    for entry in tree.iter() {
        let entry = entry
            .map_err(|e| ExtractError::Decode(format!("tree iter: {e}")))?;
        let name = entry
            .filename()
            .to_str()
            .map_err(|_| ExtractError::Decode("non-utf8 tree entry filename".into()))?
            .to_owned();
        let child_path = dst.join(&name);
        let oid = entry.object_id();

        match entry.mode().kind() {
            EntryKind::Blob | EntryKind::BlobExecutable => {
                let blob = repo
                    .find_object(oid)
                    .map_err(|e| ExtractError::Lookup(format!("blob {oid}: {e}")))?
                    .try_into_blob()
                    .map_err(|e| ExtractError::Decode(format!("blob {oid}: {e}")))?;
                if let Some(parent) = child_path.parent() {
                    fs::create_dir_all(parent).map_err(|source| ExtractError::Io {
                        path: parent.to_path_buf(),
                        source,
                    })?;
                }
                fs::write(&child_path, &blob.data).map_err(|source| ExtractError::Io {
                    path: child_path.clone(),
                    source,
                })?;
            }
            EntryKind::Tree => {
                fs::create_dir_all(&child_path).map_err(|source| ExtractError::Io {
                    path: child_path.clone(),
                    source,
                })?;
                let sub = repo
                    .find_object(oid)
                    .map_err(|e| ExtractError::Lookup(format!("tree {oid}: {e}")))?
                    .try_into_tree()
                    .map_err(|e| ExtractError::Decode(format!("tree {oid}: {e}")))?;
                write_tree_recursive(repo, &sub, &child_path)?;
            }
            // Symlinks: following them would read arbitrary host files.
            // Gitlinks (submodules): nothing sensible to materialize locally.
            EntryKind::Link | EntryKind::Commit => continue,
        }
    }
    Ok(())
}

/// Enumerate local branches (`refs/heads/*`) with their tip commit oid.
///
/// Returns the short branch name (no `refs/heads/` prefix). Skips refs
/// that fail to peel — surfacing an error for one broken ref would block
/// rendering the healthy ones.
pub fn list_local_branches(
    repo: &gix::Repository,
) -> Result<Vec<(String, ObjectId)>, EnumError> {
    let platform = repo
        .references()
        .map_err(|e| EnumError::References(e.to_string()))?;
    let iter = platform
        .local_branches()
        .map_err(|e| EnumError::References(e.to_string()))?;

    let mut out = Vec::new();
    for reference in iter {
        let Ok(mut reference) = reference else { continue };
        let name = reference.name().shorten().to_string();
        let Ok(id) = reference.peel_to_id() else { continue };
        out.push((name, id.detach()));
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub label: String,
    pub head_commit: ObjectId,
    /// Full ref name (`refs/heads/main`) if a branch is checked out; `None`
    /// for detached HEAD.
    pub ref_name: Option<String>,
}

/// Enumerate linked worktrees via `repo.worktrees()`.
///
/// The main worktree is **not** included — its state is already surfaced
/// through local-branch enumeration (the branch ref and the main worktree
/// point at the same commit). Matches `git worktree list` semantics:
/// linked worktrees are the extra ones.
///
/// Silently drops worktrees whose HEAD is unborn or unreadable — partial
/// results beat blocking the whole render on one misconfigured worktree.
pub fn list_worktrees(repo: &gix::Repository) -> Result<Vec<WorktreeInfo>, EnumError> {
    let proxies = repo
        .worktrees()
        .map_err(|e| EnumError::Worktrees(e.to_string()))?;

    let mut out = Vec::new();
    for proxy in proxies {
        let label = proxy.id().to_string();
        let Ok(wt_repo) = proxy.into_repo_with_possibly_inaccessible_worktree() else {
            continue;
        };
        let Ok(head) = wt_repo.head() else { continue };
        let info = match head.kind {
            gix::head::Kind::Symbolic(target) => {
                let full = target.name.as_bstr().to_string();
                let Ok(mut reference) = wt_repo.find_reference(&full) else { continue };
                let Ok(id) = reference.peel_to_id() else { continue };
                WorktreeInfo {
                    label,
                    head_commit: id.detach(),
                    ref_name: Some(full),
                }
            }
            gix::head::Kind::Detached { target, .. } => WorktreeInfo {
                label,
                head_commit: target,
                ref_name: None,
            },
            gix::head::Kind::Unborn(_) => continue,
        };
        out.push(info);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    // Use git(1) to build fixture repos — the commit-creation API in gix
    // 0.81 is verbose for test scaffolding and the rest of the workspace
    // already assumes git is on PATH (nix flake installs it).
    fn git(args: &[&str], cwd: &Path) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env_remove("GIT_CONFIG_GLOBAL")
            .env("HOME", cwd)
            .output()
            .expect("spawn git");
        assert!(
            out.status.success(),
            "git {args:?} failed: stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }

    fn new_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        git(&["init", "-b", "main"], tmp.path());
        git(&["config", "user.name", "test"], tmp.path());
        git(&["config", "user.email", "test@example.com"], tmp.path());
        tmp
    }

    fn write_file(root: &Path, rel: &str, body: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, body).unwrap();
    }

    fn commit_all(root: &Path, msg: &str) -> ObjectId {
        git(&["add", "-A"], root);
        git(&["commit", "-m", msg], root);
        let repo = gix::open(root).unwrap();
        repo.head_id().unwrap().detach()
    }

    // ---- extract_subtree ----

    #[test]
    fn extract_subtree_materializes_blobs_from_tree() {
        let tmp = new_repo();
        write_file(tmp.path(), ".kinora/config.styx", b"repo-url \"x\"\n");
        write_file(tmp.path(), ".kinora/store/ab/abcd", b"blob-bytes");
        write_file(tmp.path(), "README.md", b"# outside\n");
        let oid = commit_all(tmp.path(), "seed");

        let repo = gix::open(tmp.path()).unwrap();
        let dst = TempDir::new().unwrap();
        extract_subtree(&repo, oid, ".kinora", dst.path()).unwrap();

        assert_eq!(
            fs::read(dst.path().join("config.styx")).unwrap(),
            b"repo-url \"x\"\n",
        );
        assert_eq!(
            fs::read(dst.path().join("store/ab/abcd")).unwrap(),
            b"blob-bytes",
        );
        // README.md lives outside .kinora/ and must not land in dst.
        assert!(!dst.path().join("README.md").exists());
    }

    #[test]
    fn extract_subtree_errors_when_subtree_absent() {
        let tmp = new_repo();
        write_file(tmp.path(), "README.md", b"only this\n");
        let oid = commit_all(tmp.path(), "no kinora");

        let repo = gix::open(tmp.path()).unwrap();
        let dst = TempDir::new().unwrap();
        let err = extract_subtree(&repo, oid, ".kinora", dst.path()).unwrap_err();
        assert!(matches!(err, ExtractError::SubtreeAbsent { .. }), "got: {err:?}");
    }

    #[test]
    fn extract_subtree_errors_when_path_is_a_blob_not_a_tree() {
        let tmp = new_repo();
        // `.kinora` is a file here, not a directory.
        write_file(tmp.path(), ".kinora", b"oops");
        let oid = commit_all(tmp.path(), "kinora as blob");

        let repo = gix::open(tmp.path()).unwrap();
        let dst = TempDir::new().unwrap();
        let err = extract_subtree(&repo, oid, ".kinora", dst.path()).unwrap_err();
        assert!(matches!(err, ExtractError::SubtreeAbsent { .. }), "got: {err:?}");
    }

    #[test]
    fn extract_subtree_creates_dst_if_missing() {
        let tmp = new_repo();
        write_file(tmp.path(), ".kinora/config.styx", b"x");
        let oid = commit_all(tmp.path(), "seed");

        let repo = gix::open(tmp.path()).unwrap();
        let parent = TempDir::new().unwrap();
        let dst = parent.path().join("fresh");
        assert!(!dst.exists());
        extract_subtree(&repo, oid, ".kinora", &dst).unwrap();
        assert!(dst.is_dir());
        assert!(dst.join("config.styx").is_file());
    }

    // ---- list_local_branches ----

    #[test]
    fn list_local_branches_enumerates_refs_heads() {
        let tmp = new_repo();
        write_file(tmp.path(), "a.txt", b"a");
        let main = commit_all(tmp.path(), "main commit");

        git(&["checkout", "-b", "feature/x"], tmp.path());
        write_file(tmp.path(), "b.txt", b"b");
        let feat = commit_all(tmp.path(), "feature commit");

        let repo = gix::open(tmp.path()).unwrap();
        let mut branches = list_local_branches(&repo).unwrap();
        branches.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(branches.len(), 2);
        let names: Vec<&str> = branches.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["feature/x", "main"]);
        let by_name: std::collections::BTreeMap<_, _> =
            branches.iter().cloned().collect();
        assert_eq!(by_name["main"], main);
        assert_eq!(by_name["feature/x"], feat);
    }

    #[test]
    fn list_local_branches_empty_repo_is_empty_vec() {
        let tmp = TempDir::new().unwrap();
        git(&["init", "-b", "main"], tmp.path());
        let repo = gix::open(tmp.path()).unwrap();
        assert!(list_local_branches(&repo).unwrap().is_empty());
    }

    // ---- list_worktrees ----

    #[test]
    fn list_worktrees_empty_when_no_linked_worktrees() {
        let tmp = new_repo();
        write_file(tmp.path(), "a", b"a");
        commit_all(tmp.path(), "seed");
        let repo = gix::open(tmp.path()).unwrap();
        assert!(list_worktrees(&repo).unwrap().is_empty());
    }

    #[test]
    fn list_worktrees_surfaces_linked_worktree_branch() {
        let tmp = new_repo();
        write_file(tmp.path(), "a", b"a");
        let main = commit_all(tmp.path(), "seed");

        // Add a linked worktree on a new branch.
        let wt_parent = TempDir::new().unwrap();
        let wt = wt_parent.path().join("wt1");
        git(
            &[
                "worktree",
                "add",
                "-b",
                "wt-branch",
                wt.to_str().unwrap(),
            ],
            tmp.path(),
        );

        let repo = gix::open(tmp.path()).unwrap();
        let worktrees = list_worktrees(&repo).unwrap();
        assert_eq!(worktrees.len(), 1, "got: {worktrees:?}");
        let info = &worktrees[0];
        assert_eq!(info.head_commit, main);
        assert_eq!(info.ref_name.as_deref(), Some("refs/heads/wt-branch"));
    }
}
