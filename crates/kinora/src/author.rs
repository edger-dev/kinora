use std::path::Path;

/// Read `user.name` from the git config at `repo_root`, if available.
///
/// Returns `None` if the directory is not a git repo, if there is no
/// `user.name` configured, or if the config cannot be read. Kinora uses
/// this as a fallback for the ledger event's `author` field when no
/// explicit author was passed on the command line.
pub fn resolve_author_from_git(repo_root: &Path) -> Option<String> {
    let repo = gix::open(repo_root).ok()?;
    resolve_from_repo(&repo)
}

fn resolve_from_repo(repo: &gix::Repository) -> Option<String> {
    let name = repo.config_snapshot().string("user.name")?;
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Open an *isolated* repo so tests don't read the host's global git
    // config — otherwise a dev box with `user.name` set would mask the
    // "unset" test case.
    fn open_isolated(path: &Path) -> gix::Repository {
        gix::open_opts(path, gix::open::Options::isolated())
            .expect("open isolated")
    }

    fn git_init_with_user(path: &Path, user_name: Option<&str>) {
        gix::init(path).expect("gix init");
        if let Some(name) = user_name {
            let cfg = path.join(".git").join("config");
            let existing = fs::read_to_string(&cfg).unwrap_or_default();
            let appended = format!("{existing}[user]\n\tname = {name}\n");
            fs::write(&cfg, appended).unwrap();
        }
    }

    #[test]
    fn returns_none_for_non_git_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(resolve_author_from_git(tmp.path()).is_none());
    }

    #[test]
    fn returns_none_when_user_name_unset() {
        let tmp = TempDir::new().unwrap();
        git_init_with_user(tmp.path(), None);
        let repo = open_isolated(tmp.path());
        assert!(resolve_from_repo(&repo).is_none());
    }

    #[test]
    fn returns_user_name_when_configured() {
        let tmp = TempDir::new().unwrap();
        git_init_with_user(tmp.path(), Some("Test User"));
        let repo = open_isolated(tmp.path());
        assert_eq!(resolve_from_repo(&repo).as_deref(), Some("Test User"));
    }
}
