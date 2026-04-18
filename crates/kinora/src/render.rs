//! Render kinos and kinographs into an in-memory mdbook-shaped `Book`.
//!
//! The library layer has no knowledge of disk layout, branches, or git. It
//! takes a loaded `Resolver` plus a branch label, and returns a deterministic
//! list of rendered pages. The CLI layer pairs this with disk writes.
//!
//! Kind dispatch (MVP):
//! - `markdown` — content is passed through verbatim
//! - `kinograph` — composed via `Kinograph::render`
//! - `text` — wrapped in a fenced `text` code block
//! - `binary` — replaced with a placeholder note
//! - other kinds — placeholder note naming the kind
//!
//! `kino://<64hex-id>[/]` occurrences in the body are rewritten to relative
//! links to the target page. Unknown ids are left unchanged.

use std::collections::HashMap;
use std::str::FromStr;

use crate::hash::{Hash, SHORTHASH_LEN};
use crate::kinograph::{Kinograph, KinographError};
use crate::resolve::{ResolveError, Resolver};

/// One rendered page in the book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub id: String,
    pub slug: String,
    pub branch: String,
    pub title: String,
    pub kind: String,
    pub body: String,
}

/// Ordered collection of rendered pages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Book {
    pub pages: Vec<RenderedPage>,
}

#[derive(Debug)]
pub enum RenderError {
    Resolve(ResolveError),
    Kinograph(KinographError),
    Utf8(std::string::FromUtf8Error),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Resolve(e) => write!(f, "{e}"),
            RenderError::Kinograph(e) => write!(f, "{e}"),
            RenderError::Utf8(e) => write!(f, "kino content is not valid UTF-8: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<ResolveError> for RenderError {
    fn from(e: ResolveError) -> Self {
        RenderError::Resolve(e)
    }
}

impl From<KinographError> for RenderError {
    fn from(e: KinographError) -> Self {
        RenderError::Kinograph(e)
    }
}

impl From<std::string::FromUtf8Error> for RenderError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        RenderError::Utf8(e)
    }
}

/// Render every identity's current head into a `Book` labelled by `branch`.
///
/// Pages are sorted by `(name-or-empty, id)` so the output is stable across
/// runs. Identities with no head are skipped silently (shouldn't happen once
/// the ledger is well-formed, but it keeps the renderer robust).
pub fn render_for_branch(
    resolver: &Resolver,
    branch: impl Into<String>,
) -> Result<Book, RenderError> {
    let branch = branch.into();
    let mut entries: Vec<(String, String, String, String)> = Vec::new(); // (name, id, kind, body)

    let mut ids: Vec<&String> = resolver.identities().keys().collect();
    ids.sort();

    for id in ids {
        let resolved = match resolver.resolve_by_id(id) {
            Ok(r) => r,
            Err(ResolveError::MultipleHeads { .. }) => continue,
            Err(e) => return Err(RenderError::Resolve(e)),
        };
        let kind = resolved.head.kind.clone();
        let name = resolved
            .head
            .metadata
            .get("name")
            .cloned()
            .unwrap_or_default();
        let body = render_body(&kind, &resolved.content, resolver)?;
        entries.push((name, resolved.id.clone(), kind, body));
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let slug_by_id = build_slug_map(&entries);

    let pages = entries
        .into_iter()
        .map(|(name, id, kind, body)| {
            let slug = slug_by_id[&id].clone();
            let body = rewrite_kino_urls(&body, &slug_by_id);
            let title = if name.is_empty() {
                short_id(&id).to_owned()
            } else {
                name.clone()
            };
            RenderedPage {
                id,
                slug,
                branch: branch.clone(),
                title,
                kind,
                body,
            }
        })
        .collect();

    Ok(Book { pages })
}

fn render_body(
    kind: &str,
    content: &[u8],
    resolver: &Resolver,
) -> Result<String, RenderError> {
    match kind {
        "markdown" => Ok(String::from_utf8(content.to_vec())?),
        "kinograph" => {
            let kinograph = Kinograph::parse(content)?;
            Ok(kinograph.render(resolver)?)
        }
        "text" => {
            let body = String::from_utf8(content.to_vec())?;
            Ok(format!("```text\n{body}\n```\n"))
        }
        "binary" => Ok("> (opaque binary — see source store for bytes)\n".to_owned()),
        other => Ok(format!(
            "> (unrenderable kind `{other}` — no renderer registered)\n"
        )),
    }
}

fn build_slug_map(entries: &[(String, String, String, String)]) -> HashMap<String, String> {
    let mut slugs: HashMap<String, String> = HashMap::new();
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (name, id, _, _) in entries {
        let base = slug_for(name, id);
        let mut candidate = base.clone();
        let mut n = 2;
        while used.contains(&candidate) {
            candidate = format!("{base}-{n}");
            n += 1;
        }
        used.insert(candidate.clone());
        slugs.insert(id.clone(), candidate);
    }
    slugs
}

fn slug_for(name: &str, id: &str) -> String {
    let shorthash = short_id(id);
    if name.is_empty() {
        shorthash.to_owned()
    } else {
        format!("{}-{}", sanitize_slug(name), shorthash)
    }
}

fn short_id(id: &str) -> &str {
    if id.len() >= SHORTHASH_LEN {
        &id[..SHORTHASH_LEN]
    } else {
        id
    }
}

fn sanitize_slug(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_dash = false;
    for ch in raw.chars() {
        let mapped = match ch {
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            'a'..='z' | '0'..='9' | '_' | '-' => Some(ch),
            _ => None,
        };
        match mapped {
            Some(c) => {
                out.push(c);
                prev_dash = c == '-';
            }
            None => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                    prev_dash = true;
                }
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "kino".to_owned()
    } else {
        out
    }
}

/// Walk `body` and rewrite `kino://<64hex>[/]` occurrences to relative
/// markdown links. Unknown ids are left unchanged so the book still renders.
fn rewrite_kino_urls(body: &str, slug_by_id: &HashMap<String, String>) -> String {
    const PREFIX: &str = "kino://";
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(idx) = rest.find(PREFIX) {
        out.push_str(&rest[..idx]);
        let after_prefix = &rest[idx + PREFIX.len()..];
        if after_prefix.len() >= 64 && after_prefix.is_char_boundary(64) {
            let id_slice = &after_prefix[..64];
            if Hash::from_str(id_slice).is_ok()
                && let Some(slug) = slug_by_id.get(id_slice)
            {
                out.push_str(slug);
                out.push_str(".md");
                let after_id = &after_prefix[64..];
                let skip_slash = if after_id.starts_with('/') { 1 } else { 0 };
                rest = &after_id[skip_slash..];
                continue;
            }
        }
        out.push_str(PREFIX);
        rest = after_prefix;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::init;
    use crate::kino::{store_kino, StoreKinoParams};
    use crate::paths::kinora_root;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn setup() -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        init(tmp.path(), "https://example.com/x.git").unwrap();
        let root = kinora_root(tmp.path());
        (tmp, root)
    }

    fn params(kind: &str, content: &[u8], name: &str) -> StoreKinoParams {
        StoreKinoParams {
            kind: kind.into(),
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
    fn renders_single_markdown_kino() {
        let (_t, root) = setup();
        store_kino(&root, params("markdown", b"# Hello\n\nBody text.", "greet")).unwrap();
        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        assert_eq!(book.pages.len(), 1);
        let page = &book.pages[0];
        assert_eq!(page.branch, "main");
        assert_eq!(page.kind, "markdown");
        assert_eq!(page.title, "greet");
        assert!(page.slug.starts_with("greet-"));
        assert!(page.body.contains("# Hello"));
    }

    #[test]
    fn pages_sorted_by_name_then_id_for_stability() {
        let (_t, root) = setup();
        store_kino(&root, params("markdown", b"b", "beta")).unwrap();
        store_kino(&root, params("markdown", b"a", "alpha")).unwrap();
        store_kino(&root, params("markdown", b"c", "charlie")).unwrap();
        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        let titles: Vec<_> = book.pages.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, vec!["alpha", "beta", "charlie"]);
    }

    #[test]
    fn text_kind_wraps_in_fenced_code_block() {
        let (_t, root) = setup();
        store_kino(&root, params("text", b"plain body", "note")).unwrap();
        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        assert!(book.pages[0].body.starts_with("```text\n"));
        assert!(book.pages[0].body.contains("plain body"));
        assert!(book.pages[0].body.trim_end().ends_with("```"));
    }

    #[test]
    fn binary_kind_emits_placeholder() {
        let (_t, root) = setup();
        store_kino(&root, params("binary", b"\x00\x01\x02", "blob")).unwrap();
        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        assert!(
            book.pages[0].body.contains("opaque binary"),
            "got body: {}",
            book.pages[0].body
        );
    }

    #[test]
    fn unknown_kind_emits_warning_placeholder() {
        let (_t, root) = setup();
        store_kino(&root, params("mystery::format", b"x", "m")).unwrap();
        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        assert!(
            book.pages[0].body.contains("unrenderable kind"),
            "got body: {}",
            book.pages[0].body
        );
        assert!(book.pages[0].body.contains("mystery::format"));
    }

    #[test]
    fn kinograph_kind_renders_composed_content() {
        let (_t, root) = setup();
        let a = store_kino(&root, params("markdown", b"alpha", "a")).unwrap();
        let b = store_kino(&root, params("markdown", b"bravo", "b")).unwrap();

        let kg_content = format!("entries ({{id {}}} {{id {}}})", a.event.id, b.event.id);
        store_kino(
            &root,
            params("kinograph", kg_content.as_bytes(), "composed"),
        )
        .unwrap();

        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        let kg_page = book.pages.iter().find(|p| p.kind == "kinograph").unwrap();
        assert!(kg_page.body.contains("alpha"));
        assert!(kg_page.body.contains("bravo"));
    }

    #[test]
    fn kino_url_rewritten_to_relative_md_link() {
        let (_t, root) = setup();
        let target = store_kino(&root, params("markdown", b"target body", "target")).unwrap();
        let referrer_body = format!(
            "See also: [target](kino://{}/) for details.\n",
            target.event.id
        );
        store_kino(
            &root,
            params("markdown", referrer_body.as_bytes(), "referrer"),
        )
        .unwrap();

        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        let referrer_page = book.pages.iter().find(|p| p.title == "referrer").unwrap();
        let target_slug = &book.pages.iter().find(|p| p.title == "target").unwrap().slug;
        assert!(
            referrer_page.body.contains(&format!("{target_slug}.md")),
            "expected body to contain `{target_slug}.md`; got: {}",
            referrer_page.body
        );
        assert!(
            !referrer_page.body.contains("kino://"),
            "kino:// URL should have been rewritten: {}",
            referrer_page.body
        );
    }

    #[test]
    fn kino_url_with_unknown_id_passes_through_untouched() {
        let (_t, root) = setup();
        let bogus = "0".repeat(64);
        let body = format!("broken: [x](kino://{bogus}/)\n");
        store_kino(&root, params("markdown", body.as_bytes(), "x")).unwrap();

        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        assert!(book.pages[0].body.contains(&format!("kino://{bogus}/")));
    }

    #[test]
    fn kino_url_without_trailing_slash_also_rewritten() {
        let (_t, root) = setup();
        let target = store_kino(&root, params("markdown", b"t", "target")).unwrap();
        let body = format!("link: kino://{}\n", target.event.id);
        store_kino(&root, params("markdown", body.as_bytes(), "ref")).unwrap();

        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        let referrer = book.pages.iter().find(|p| p.title == "ref").unwrap();
        assert!(!referrer.body.contains("kino://"));
    }

    #[test]
    fn empty_repo_yields_empty_book() {
        let (_t, root) = setup();
        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        assert!(book.pages.is_empty());
    }

    #[test]
    fn branch_label_propagates_to_every_page() {
        let (_t, root) = setup();
        store_kino(&root, params("markdown", b"x", "a")).unwrap();
        store_kino(&root, params("markdown", b"y", "b")).unwrap();
        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "feature/foo").unwrap();
        assert!(book.pages.iter().all(|p| p.branch == "feature/foo"));
    }

    #[test]
    fn slugs_are_unique_when_names_collide() {
        // Two identities with the same metadata.name — shorthash suffix
        // should keep slugs unique.
        let (_t, root) = setup();
        store_kino(&root, params("markdown", b"a", "dup")).unwrap();
        store_kino(&root, params("markdown", b"b", "dup")).unwrap();
        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        let slugs: std::collections::HashSet<_> =
            book.pages.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs.len(), book.pages.len(), "slugs collided: {book:?}");
    }

    #[test]
    fn forked_identities_are_skipped() {
        // An identity with multiple heads would fail `resolve_by_id`; the
        // renderer skips it instead of blowing up the whole book.
        let (_t, root) = setup();
        let birth = store_kino(&root, params("markdown", b"v1", "forked")).unwrap();
        let mut a = params("markdown", b"left", "forked");
        a.id = Some(birth.event.id.clone());
        a.parents = vec![birth.event.hash.clone()];
        a.ts = "2026-04-18T10:00:01Z".into();
        store_kino(&root, a).unwrap();

        // Mint a sibling head in a fresh lineage.
        std::fs::remove_file(crate::paths::head_path(&root)).unwrap();
        let mut b = params("markdown", b"right", "forked");
        b.id = Some(birth.event.id.clone());
        b.parents = vec![birth.event.hash.clone()];
        b.ts = "2026-04-18T10:00:02Z".into();
        store_kino(&root, b).unwrap();

        // Add a second, non-forked identity — it should still render.
        store_kino(&root, params("markdown", b"ok", "clean")).unwrap();

        let resolver = Resolver::load(&root).unwrap();
        let book = render_for_branch(&resolver, "main").unwrap();
        let titles: Vec<_> = book.pages.iter().map(|p| p.title.as_str()).collect();
        assert!(titles.contains(&"clean"));
        // Note: the branch-aware tiebreaker may or may not pick a head for the
        // forked identity depending on which lineage HEAD points at. The key
        // guarantee is that the render does not error on it.
    }
}
