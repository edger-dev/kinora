//! A mirror named by URL — `s3://bucket/prefix` (keys and endpoint from the
//! environment or explicit options), `file:///dir`, `memory://` — and a
//! blocking face for sync callers, safe to call from inside an async runtime.
//!
//! implements: b4e072636ef26d99d9b0f32227da47d9c11e93d8b458e4631af57c4c31a9ba57@e7980f316714ef8cacb7d4f87058a644fde9a63d22ac4ca01bc828a99029e6b1

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::PathBuf;

use kinora::kino::{StoreKinoParams, store_kino};
use kinora::paths::kinora_root;
use kinora::resolve::Resolver;
use kinora_remote::{Mirror, RemoteError, blocking};

fn store_with(name: &str, bodies: &[&str]) -> PathBuf {
    let repo =
        std::env::temp_dir().join(format!("kinora-remote-cfg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    kinora::init::init(&repo, "https://example.org/r").unwrap();
    let root = kinora_root(&repo);
    for body in bodies {
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
    }
    root
}

fn ids(root: &std::path::Path) -> usize {
    Resolver::load(root).unwrap().identities().len()
}

#[test]
fn an_s3_url_names_the_bucket_and_prefix_with_options_for_the_rest() {
    let mirror = Mirror::from_url_with(
        "s3://kudo-contexts/yj/personal",
        [
            ("aws_access_key_id", "AKIDEXAMPLE"),
            ("aws_secret_access_key", "secret"),
            ("aws_endpoint", "https://account.r2.cloudflarestorage.com"),
            ("aws_region", "auto"),
        ],
    )
    .unwrap();
    assert_eq!(mirror.prefix(), "yj/personal");
}

#[test]
fn an_unknown_scheme_is_refused() {
    assert!(matches!(
        Mirror::from_url_with("ftp://host/x", Vec::<(&str, &str)>::new()),
        Err(RemoteError::Url(_))
    ));
    assert!(matches!(
        Mirror::from_url_with("not a url", Vec::<(&str, &str)>::new()),
        Err(RemoteError::Url(_))
    ));
}

#[test]
fn a_file_mirror_round_trips_through_the_blocking_face() {
    let bucket =
        std::env::temp_dir().join(format!("kinora-remote-cfg-bucket-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&bucket);
    let url = format!("file://{}", bucket.display());
    let a = store_with("file-a", &["# a\n"]);
    let b = store_with("file-b", &["# b\n"]);

    let mirror = Mirror::from_url(&url).unwrap();
    blocking::push(&mirror, &a).unwrap();
    blocking::push(&mirror, &b).unwrap();
    blocking::pull(&mirror, &a).unwrap();
    blocking::pull(&mirror, &b).unwrap();
    assert_eq!(ids(&a), 2);
    assert_eq!(ids(&b), 2);
}

/// kudo calls the blocking face from tool handlers running on a tokio
/// runtime: it must not panic there.
#[tokio::test]
async fn the_blocking_face_works_inside_an_async_runtime() {
    let a = store_with("in-rt", &["# in a runtime\n"]);
    let mirror = Mirror::from_url("memory://").unwrap();
    let report = blocking::push(&mirror, &a).unwrap();
    assert!(report.copied > 0);
}

/// An unreachable S3 endpoint fails in seconds, not after a minute of
/// retries: kudo's hooks fall back to working locally and must not stall.
#[test]
fn an_unreachable_s3_endpoint_fails_fast() {
    let mirror = Mirror::from_url_with(
        "s3://bucket/prefix",
        [
            ("aws_access_key_id", "k"),
            ("aws_secret_access_key", "s"),
            ("aws_endpoint", "http://10.255.255.1:9"),
            ("aws_region", "auto"),
            ("aws_allow_http", "true"),
        ],
    )
    .unwrap();
    let root = store_with("unreachable", &["# x\n"]);
    let started = std::time::Instant::now();
    assert!(blocking::pull(&mirror, &root).is_err());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "took {:?}",
        started.elapsed()
    );
}

/// A mirror that refuses this machine — bad keys, no permission — says so,
/// whichever way the store reports it, so callers never mistake it for an
/// outage.
#[test]
fn a_refusal_is_told_apart_from_an_outage() {
    let refused = [
        object_store::Error::PermissionDenied {
            path: "p".into(),
            source: "denied".into(),
        },
        object_store::Error::Unauthenticated {
            path: "p".into(),
            source: "who".into(),
        },
        object_store::Error::Generic {
            store: "S3",
            source: "Server returned non-2xx status code: 403 Forbidden: SignatureDoesNotMatch"
                .into(),
        },
        object_store::Error::Generic {
            store: "S3",
            source: "Server returned non-2xx status code: 401 Unauthorized".into(),
        },
    ];
    for e in refused {
        assert!(RemoteError::Store(e).is_refused());
    }
    let outage = object_store::Error::Generic {
        store: "S3",
        source: "error sending request: connection refused".into(),
    };
    assert!(!RemoteError::Store(outage).is_refused());
    assert!(!RemoteError::Io(std::io::Error::other("disk")).is_refused());
}
