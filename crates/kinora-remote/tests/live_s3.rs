//! A round trip against a real S3 endpoint (MinIO for development, R2 for
//! use). Runs only when `KINORA_REMOTE_S3_TEST` names a mirror URL, with the
//! `AWS_*` variables for it; skipped otherwise.
//!
//! implements: b4e072636ef26d99d9b0f32227da47d9c11e93d8b458e4631af57c4c31a9ba57@e7980f316714ef8cacb7d4f87058a644fde9a63d22ac4ca01bc828a99029e6b1

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kinora::kino::{StoreKinoParams, store_kino};
use kinora::paths::kinora_root;
use kinora::resolve::Resolver;
use kinora_remote::{Mirror, blocking};

fn store_with(name: &str, body: &str) -> std::path::PathBuf {
    let repo =
        std::env::temp_dir().join(format!("kinora-remote-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    kinora::init::init(&repo, "https://example.org/r").unwrap();
    let root = kinora_root(&repo);
    store_kino(
        &root,
        StoreKinoParams {
            kind: "markdown".into(),
            content: body.as_bytes().to_vec(),
            author: "test".into(),
            provenance: "test".into(),
            ts: "2026-10-21T00:00:00Z".into(),
            metadata: Default::default(),
            id: None,
            parents: Vec::new(),
        },
    )
    .unwrap();
    root
}

#[test]
fn a_real_s3_mirror_round_trips_and_never_overwrites() {
    let Ok(base) = std::env::var("KINORA_REMOTE_S3_TEST") else {
        eprintln!("skipped: KINORA_REMOTE_S3_TEST is not set");
        return;
    };
    let url = format!("{}/run-{}", base.trim_end_matches('/'), std::process::id());
    let mirror = Mirror::from_url(&url).unwrap();
    let a = store_with("a", "# from a, live\n");
    let b = store_with("b", "# from b, live\n");

    let sent = blocking::push(&mirror, &a).unwrap();
    assert!(sent.copied > 0, "{sent:?}");
    blocking::push(&mirror, &b).unwrap();
    blocking::pull(&mirror, &a).unwrap();
    blocking::pull(&mirror, &b).unwrap();
    for root in [&a, &b] {
        assert_eq!(Resolver::load(root).unwrap().identities().len(), 2);
    }
    // Every file is already there: a full push puts nothing over it.
    let again = blocking::push_all(&mirror, &a).unwrap();
    assert_eq!(again.copied, 0, "{again:?}");
}
