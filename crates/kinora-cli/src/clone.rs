use std::path::{Path, PathBuf};

use kinora::clone::{clone_repo, CloneParams, CloneReport};

use crate::common::CliError;

/// Default value for both the author and provenance fields of
/// [`CloneParams`] when the caller omits them. Clone doesn't derive
/// author from git — it's a local rebuild that may run outside a git
/// worktree — so falling back to a literal keeps the contract local and
/// doesn't fail on hosts without `git user.name` configured.
pub const DEFAULT_PROVENANCE: &str = "clone";

pub struct CloneRunArgs {
    pub src: String,
    pub dst: String,
    pub author: Option<String>,
    pub provenance: Option<String>,
}

#[derive(Debug)]
pub struct CloneRunReport {
    pub inner: CloneReport,
    pub src: PathBuf,
    pub dst: PathBuf,
}

/// Run `kinora clone`.
///
/// `src` and `dst` are taken verbatim as paths to `.kinora/` directories:
/// clone does not walk up looking for a repo root. Relative paths are
/// resolved against `cwd`.
pub fn run_clone(cwd: &Path, args: CloneRunArgs) -> Result<CloneRunReport, CliError> {
    let src = resolve_path(cwd, &args.src);
    let dst = resolve_path(cwd, &args.dst);

    let author = args.author.unwrap_or_else(|| DEFAULT_PROVENANCE.to_owned());
    let provenance = args.provenance.unwrap_or_else(|| DEFAULT_PROVENANCE.to_owned());
    let ts = jiff::Timestamp::now().to_string();

    let params = CloneParams { author, provenance, ts };
    let inner = clone_repo(&src, &dst, params)?;
    Ok(CloneRunReport { inner, src, dst })
}

fn resolve_path(cwd: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

/// One-screen human summary printed after `kinora clone` succeeds.
///
/// Mirrors the `CloneReport` counters: kinos rebuilt, unreachable blobs
/// dropped, and filenames rewritten into the canonical `<hash>.<ext>`
/// form.
pub fn format_clone_summary(r: &CloneRunReport) -> String {
    let inner = &r.inner;
    let mut out = String::new();
    out.push_str(&format!(
        "cloned {} -> {}\n",
        r.src.display(),
        r.dst.display(),
    ));
    out.push_str(&format!(
        "{} kino{} rebuilt\n",
        inner.kinos_rebuilt,
        plural_s(inner.kinos_rebuilt),
    ));
    out.push_str(&format!(
        "{} blob{} dropped (unreachable)\n",
        inner.blobs_dropped,
        plural_s(inner.blobs_dropped),
    ));
    out.push_str(&format!(
        "{} filename{} rewritten",
        inner.filenames_rewritten,
        plural_s(inner.filenames_rewritten),
    ));
    out
}

fn plural_s(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinora::init::init;
    use kinora::paths::kinora_root;
    use tempfile::TempDir;

    fn src_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), "https://example.com/x.git").unwrap();
        tmp
    }

    fn args(src: &Path, dst: &Path) -> CloneRunArgs {
        CloneRunArgs {
            src: src.to_string_lossy().into_owned(),
            dst: dst.to_string_lossy().into_owned(),
            author: Some("yj".into()),
            provenance: Some("cli-test".into()),
        }
    }

    #[test]
    fn run_clone_succeeds_on_empty_repo() {
        let src = src_repo();
        let src_kin = kinora_root(src.path());
        let dst = TempDir::new().unwrap();
        let dst_kin = dst.path().join(".kinora2");
        let r = run_clone(
            std::env::current_dir().unwrap().as_path(),
            args(&src_kin, &dst_kin),
        )
        .unwrap();
        assert_eq!(r.inner.kinos_rebuilt, 0);
        assert_eq!(r.inner.blobs_dropped, 0);
        assert_eq!(r.inner.filenames_rewritten, 0);
        assert!(dst_kin.join("config.styx").is_file());
    }

    #[test]
    fn run_clone_errors_when_src_not_kinora_dir() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        let dst_kin = dst.path().join(".kinora2");
        let err = run_clone(
            std::env::current_dir().unwrap().as_path(),
            args(src.path(), &dst_kin),
        )
        .unwrap_err();
        assert!(matches!(err, CliError::Clone(_)), "got: {err:?}");
    }

    #[test]
    fn run_clone_errors_when_dst_not_empty() {
        let src = src_repo();
        let src_kin = kinora_root(src.path());
        let dst = TempDir::new().unwrap();
        // dst has a stray file, so not empty
        std::fs::write(dst.path().join("stray"), b"x").unwrap();
        let err = run_clone(
            std::env::current_dir().unwrap().as_path(),
            args(&src_kin, dst.path()),
        )
        .unwrap_err();
        assert!(matches!(err, CliError::Clone(_)), "got: {err:?}");
    }

    #[test]
    fn run_clone_resolves_relative_paths_against_cwd() {
        let src = src_repo();
        let src_kin = kinora_root(src.path());
        // cwd = src.path(); src relative = ".kinora"; dst relative = "out"
        let a = CloneRunArgs {
            src: ".kinora".into(),
            dst: "out".into(),
            author: Some("yj".into()),
            provenance: Some("cli-test".into()),
        };
        let r = run_clone(src.path(), a).unwrap();
        assert_eq!(r.src, src_kin);
        assert_eq!(r.dst, src.path().join("out"));
        assert!(r.dst.join("config.styx").is_file());
    }

    #[test]
    fn run_clone_provenance_defaults_to_clone_when_omitted() {
        let src = src_repo();
        let src_kin = kinora_root(src.path());
        let dst = TempDir::new().unwrap();
        let dst_kin = dst.path().join(".kinora2");
        let mut a = args(&src_kin, &dst_kin);
        a.author = None;
        a.provenance = None;
        // Empty repo — we just need it to not panic with defaults.
        run_clone(
            std::env::current_dir().unwrap().as_path(),
            a,
        )
        .unwrap();
    }

    #[test]
    fn format_summary_zero_counts_singular_forms() {
        let r = CloneRunReport {
            inner: CloneReport::default(),
            src: PathBuf::from("/a/.kinora"),
            dst: PathBuf::from("/b/.kinora"),
        };
        let s = format_clone_summary(&r);
        assert!(s.contains("cloned /a/.kinora -> /b/.kinora"), "got: {s}");
        assert!(s.contains("0 kinos rebuilt"), "got: {s}");
        assert!(s.contains("0 blobs dropped"), "got: {s}");
        assert!(s.contains("0 filenames rewritten"), "got: {s}");
    }

    #[test]
    fn format_summary_singular_forms_for_one() {
        let r = CloneRunReport {
            inner: CloneReport {
                kinos_rebuilt: 1,
                blobs_dropped: 1,
                filenames_rewritten: 1,
            },
            src: PathBuf::from("/a"),
            dst: PathBuf::from("/b"),
        };
        let s = format_clone_summary(&r);
        assert!(s.contains("1 kino rebuilt"), "got: {s}");
        assert!(s.contains("1 blob dropped"), "got: {s}");
        assert!(s.contains("1 filename rewritten"), "got: {s}");
    }

    #[test]
    fn format_summary_plural_forms_for_many() {
        let r = CloneRunReport {
            inner: CloneReport {
                kinos_rebuilt: 5,
                blobs_dropped: 3,
                filenames_rewritten: 2,
            },
            src: PathBuf::from("/a"),
            dst: PathBuf::from("/b"),
        };
        let s = format_clone_summary(&r);
        assert!(s.contains("5 kinos rebuilt"), "got: {s}");
        assert!(s.contains("3 blobs dropped"), "got: {s}");
        assert!(s.contains("2 filenames rewritten"), "got: {s}");
    }
}
