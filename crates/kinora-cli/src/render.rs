use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kinora::cache_path::CachePath;
use kinora::config::Config;
use kinora::paths::{config_path, kinora_root};
use kinora::render::{render, write_book};
use kinora::resolve::Resolver;

use crate::common::{find_repo_root, CliError};

pub struct RenderRunArgs {
    pub cache_dir: Option<String>,
}

#[derive(Debug)]
pub struct RenderReport {
    pub cache_path: PathBuf,
    pub page_count: usize,
    pub skipped_count: usize,
}

pub fn run_render(cwd: &Path, args: RenderRunArgs) -> Result<RenderReport, CliError> {
    let repo_root = find_repo_root(cwd)?;
    let kin_root = kinora_root(&repo_root);

    let config_text = std::fs::read_to_string(config_path(&kin_root))?;
    let config = Config::from_styx(&config_text)?;

    let cache = CachePath::from_repo_url(&config.repo_url);
    let cache_path = match args.cache_dir {
        Some(override_dir) => PathBuf::from(override_dir),
        None => {
            let xdg = std::env::var("XDG_CACHE_HOME").ok();
            let home = std::env::var("HOME").ok();
            resolve_cache_root(xdg.as_deref(), home.as_deref())?.join(cache.subdir())
        }
    };

    let resolver = Resolver::load(&kin_root)?;
    let owners = build_owners_map(&kin_root)?;
    let book = render(&resolver, &owners, "unreferenced")?;
    let page_count = book.pages.len();
    let skipped_count = book.skipped.len();

    let title = if cache.name.is_empty() {
        "kinora".to_owned()
    } else {
        cache.name.clone()
    };
    write_book(&cache_path, &title, &book)?;

    Ok(RenderReport { cache_path, page_count, skipped_count })
}

/// Pure resolver so the XDG/HOME branching can be unit-tested without
/// touching process env.
fn resolve_cache_root(xdg: Option<&str>, home: Option<&str>) -> Result<PathBuf, CliError> {
    if let Some(x) = xdg
        && !x.is_empty()
    {
        return Ok(PathBuf::from(x).join("kinora"));
    }
    if let Some(h) = home
        && !h.is_empty()
    {
        return Ok(PathBuf::from(h).join(".cache").join("kinora"));
    }
    Err(CliError::CacheHomeUnresolved)
}

/// Build a map from kino id to the name of the root that owns it.
///
/// Scans `.kinora/roots/` pointer files; for each pointer, loads the
/// referenced root kinograph blob and records every entry id under that
/// root's name. Kinos that are not owned by any compacted root are left
/// out — callers should fall back to a default label for them.
fn build_owners_map(_kin_root: &Path) -> Result<HashMap<String, String>, CliError> {
    // STUB (ml4t-commit-1): always returns empty. Commit 2 replaces this
    // with the real pointer-file scan + RootKinograph parse.
    Ok(HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinora::compact::{compact, CompactParams};
    use kinora::init::init;
    use kinora::kino::{store_kino, StoreKinoParams};
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), "https://github.com/edger-dev/kinora").unwrap();
        tmp
    }

    fn compact_params() -> CompactParams {
        CompactParams {
            author: "yj".into(),
            provenance: "test".into(),
            ts: "2026-04-19T10:00:00Z".into(),
        }
    }

    fn params(content: &[u8], name: &str) -> StoreKinoParams {
        StoreKinoParams {
            kind: "markdown".into(),
            content: content.to_vec(),
            author: "yj".into(),
            provenance: "test".into(),
            ts: "2026-04-18T10:00:00Z".into(),
            metadata: BTreeMap::from([("name".into(), name.into())]),
            id: None,
            parents: vec![],
        }
    }

    #[test]
    fn render_writes_pages_under_override_cache_dir() {
        let tmp = repo();
        let kin = kinora_root(tmp.path());
        store_kino(&kin, params(b"# hello\n", "greet")).unwrap();

        let cache = TempDir::new().unwrap();
        let args = RenderRunArgs {
            cache_dir: Some(cache.path().to_string_lossy().into_owned()),
        };
        let report = run_render(tmp.path(), args).unwrap();
        assert_eq!(report.cache_path, cache.path());
        assert_eq!(report.page_count, 1);
        assert!(cache.path().join("book.toml").is_file());
        assert!(cache.path().join("src/SUMMARY.md").is_file());
    }

    #[test]
    fn render_errors_when_run_outside_kinora_repo() {
        let tmp = TempDir::new().unwrap();
        let cache = TempDir::new().unwrap();
        let args = RenderRunArgs {
            cache_dir: Some(cache.path().to_string_lossy().into_owned()),
        };
        let err = run_render(tmp.path(), args).unwrap_err();
        assert!(matches!(err, CliError::NotInKinoraRepo { .. }));
    }

    #[test]
    fn render_over_existing_output_overwrites() {
        let tmp = repo();
        let kin = kinora_root(tmp.path());
        store_kino(&kin, params(b"v1", "doc")).unwrap();
        let cache = TempDir::new().unwrap();

        let args = RenderRunArgs {
            cache_dir: Some(cache.path().to_string_lossy().into_owned()),
        };
        run_render(tmp.path(), args).unwrap();

        let stale = cache.path().join("src").join("stale.md");
        std::fs::write(&stale, "stale").unwrap();
        assert!(stale.exists());

        let args = RenderRunArgs {
            cache_dir: Some(cache.path().to_string_lossy().into_owned()),
        };
        run_render(tmp.path(), args).unwrap();
        assert!(!stale.exists());
    }

    #[test]
    fn resolve_cache_root_prefers_xdg_over_home() {
        let got = resolve_cache_root(Some("/xdg"), Some("/home")).unwrap();
        assert_eq!(got, PathBuf::from("/xdg/kinora"));
    }

    #[test]
    fn resolve_cache_root_falls_back_to_home_when_xdg_absent() {
        assert_eq!(
            resolve_cache_root(None, Some("/home/user")).unwrap(),
            PathBuf::from("/home/user/.cache/kinora"),
        );
    }

    #[test]
    fn resolve_cache_root_ignores_empty_env_values() {
        assert_eq!(
            resolve_cache_root(Some(""), Some("/home/user")).unwrap(),
            PathBuf::from("/home/user/.cache/kinora"),
        );
    }

    #[test]
    fn resolve_cache_root_errors_when_nothing_resolves() {
        let err = resolve_cache_root(None, None).unwrap_err();
        assert!(matches!(err, CliError::CacheHomeUnresolved));
    }

    #[test]
    fn render_empty_repo_produces_empty_book() {
        let tmp = repo();
        let cache = TempDir::new().unwrap();
        let args = RenderRunArgs {
            cache_dir: Some(cache.path().to_string_lossy().into_owned()),
        };
        let report = run_render(tmp.path(), args).unwrap();
        assert_eq!(report.page_count, 0);
        let summary =
            std::fs::read_to_string(cache.path().join("src/SUMMARY.md")).unwrap();
        assert!(summary.starts_with("# Summary"));
    }

    // ------------------------------------------------------------------
    // build_owners_map
    // ------------------------------------------------------------------

    #[test]
    fn build_owners_map_empty_when_no_roots_dir() {
        let tmp = repo();
        let kin = kinora_root(tmp.path());
        let owners = build_owners_map(&kin).unwrap();
        assert!(owners.is_empty(), "expected empty map, got: {owners:?}");
    }

    #[test]
    fn build_owners_map_maps_entries_to_root_name() {
        let tmp = repo();
        let kin = kinora_root(tmp.path());

        let ev1 = store_kino(&kin, params(b"alpha", "alpha")).unwrap();
        let ev2 = store_kino(&kin, params(b"beta", "beta")).unwrap();

        compact(&kin, "main", compact_params()).unwrap();

        let owners = build_owners_map(&kin).unwrap();
        assert_eq!(owners.get(&ev1.event.id).map(String::as_str), Some("main"));
        assert_eq!(owners.get(&ev2.event.id).map(String::as_str), Some("main"));
    }

    // ------------------------------------------------------------------
    // End-to-end render grouping
    // ------------------------------------------------------------------

    #[test]
    fn render_pure_hot_repo_groups_under_unreferenced() {
        let tmp = repo();
        let kin = kinora_root(tmp.path());
        store_kino(&kin, params(b"# a\n", "alpha")).unwrap();
        store_kino(&kin, params(b"# b\n", "beta")).unwrap();

        let cache = TempDir::new().unwrap();
        let args = RenderRunArgs {
            cache_dir: Some(cache.path().to_string_lossy().into_owned()),
        };
        let report = run_render(tmp.path(), args).unwrap();
        assert_eq!(report.page_count, 2);
        assert!(cache.path().join("src/unreferenced/index.md").is_file());
        assert!(!cache.path().join("src/main").exists());
    }

    #[test]
    fn render_compacted_main_groups_under_main() {
        let tmp = repo();
        let kin = kinora_root(tmp.path());
        store_kino(&kin, params(b"# a\n", "alpha")).unwrap();
        store_kino(&kin, params(b"# b\n", "beta")).unwrap();
        compact(&kin, "main", compact_params()).unwrap();

        let cache = TempDir::new().unwrap();
        let args = RenderRunArgs {
            cache_dir: Some(cache.path().to_string_lossy().into_owned()),
        };
        let report = run_render(tmp.path(), args).unwrap();
        assert_eq!(report.page_count, 2);
        assert!(cache.path().join("src/main/index.md").is_file());
        assert!(!cache.path().join("src/unreferenced").exists());
    }

    #[test]
    fn render_mixed_repo_splits_between_main_and_unreferenced() {
        let tmp = repo();
        let kin = kinora_root(tmp.path());
        store_kino(&kin, params(b"# a\n", "alpha")).unwrap();
        store_kino(&kin, params(b"# b\n", "beta")).unwrap();
        compact(&kin, "main", compact_params()).unwrap();

        // Add a post-compact kino that isn't owned by any root yet.
        store_kino(&kin, params(b"# c\n", "gamma")).unwrap();

        let cache = TempDir::new().unwrap();
        let args = RenderRunArgs {
            cache_dir: Some(cache.path().to_string_lossy().into_owned()),
        };
        let report = run_render(tmp.path(), args).unwrap();
        assert_eq!(report.page_count, 3);
        assert!(cache.path().join("src/main/index.md").is_file());
        assert!(cache.path().join("src/unreferenced/index.md").is_file());
    }
}
