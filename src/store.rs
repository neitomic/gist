use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use tokio::fs;
use unicode_normalization::UnicodeNormalization;

pub const DEFAULT_PROJECT: &str = "inbox";

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default = "default_project")]
    pub project: String,
    pub slug: String,
    pub title: String,
    pub content_type: String,
    pub bytes: u64,
    #[serde(with = "time::serde::rfc3339")]
    pub created: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated: OffsetDateTime,
}

fn default_project() -> String {
    DEFAULT_PROJECT.to_string()
}

impl Meta {
    pub fn href(&self) -> String {
        format!("/d/{}/{}", self.project, self.slug)
    }
}

#[derive(Debug)]
pub enum StoreError {
    BadSlug,
    NotFound,
    Io(std::io::Error),
    Meta(serde_json::Error),
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            StoreError::NotFound
        } else {
            StoreError::Io(e)
        }
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Meta(e)
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::BadSlug => write!(f, "invalid project or slug"),
            StoreError::NotFound => write!(f, "not found"),
            StoreError::Io(e) => write!(f, "{e}"),
            StoreError::Meta(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn validate_name(name: &str) -> Result<(), StoreError> {
        if name.is_empty() || name.len() > 128 {
            return Err(StoreError::BadSlug);
        }
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return Err(StoreError::BadSlug);
        };
        if !first.is_ascii_alphanumeric() {
            return Err(StoreError::BadSlug);
        }
        if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
            return Err(StoreError::BadSlug);
        }
        if name.contains("..") {
            return Err(StoreError::BadSlug);
        }
        Ok(())
    }

    pub fn validate_slug(slug: &str) -> Result<(), StoreError> {
        Self::validate_name(slug)
    }

    pub fn validate_project(project: &str) -> Result<(), StoreError> {
        Self::validate_name(project)
    }

    /// Keep a slug that is already URL-safe. Otherwise fold accents and
    /// spaces (`Tiếng Việt.md` → `tieng-viet.md`). Path tricks (`..`, `/`)
    /// stay rejected.
    pub fn normalize_slug(name: &str) -> Option<String> {
        let name = name.trim();
        if Self::validate_slug(name).is_ok() {
            return Some(name.to_string());
        }
        if name.contains("..") || name.contains('/') || name.contains('\\') {
            return None;
        }
        ascii_slug(name)
    }

    fn nested_dir(&self, project: &str, slug: &str) -> Result<PathBuf, StoreError> {
        Self::validate_project(project)?;
        Self::validate_slug(slug)?;
        Ok(self.root.join(project).join(slug))
    }

    /// Prefer `data/{project}/{slug}`. Old layout `data/{slug}` counts as inbox.
    async fn dir(&self, project: &str, slug: &str) -> Result<PathBuf, StoreError> {
        let nested = self.nested_dir(project, slug)?;
        if tokio::fs::try_exists(nested.join("meta.json"))
            .await
            .unwrap_or(false)
        {
            return Ok(nested);
        }
        if project == DEFAULT_PROJECT {
            let legacy = self.root.join(slug);
            if tokio::fs::try_exists(legacy.join("meta.json"))
                .await
                .unwrap_or(false)
            {
                return Ok(legacy);
            }
        }
        Ok(nested)
    }

    pub async fn put(
        &self,
        project: &str,
        slug: &str,
        title: String,
        content_type: String,
        body: &[u8],
    ) -> Result<Meta, StoreError> {
        let dir = self.dir(project, slug).await?;
        fs::create_dir_all(&dir).await?;

        let now = OffsetDateTime::now_utc();
        let meta_path = dir.join("meta.json");
        let created = match fs::read(&meta_path).await {
            Ok(bytes) => serde_json::from_slice::<Meta>(&bytes)
                .map(|m| m.created)
                .unwrap_or(now),
            Err(_) => now,
        };

        let tmp = dir.join("content.tmp");
        fs::write(&tmp, body).await?;
        fs::rename(tmp, dir.join("content")).await?;

        let meta = Meta {
            project: project.to_string(),
            slug: slug.to_string(),
            title,
            content_type,
            bytes: body.len() as u64,
            created,
            updated: now,
        };
        let meta_tmp = dir.join("meta.tmp");
        fs::write(&meta_tmp, serde_json::to_vec_pretty(&meta)?).await?;
        fs::rename(meta_tmp, meta_path).await?;
        Ok(meta)
    }

    pub async fn get_meta(&self, project: &str, slug: &str) -> Result<Meta, StoreError> {
        let dir = self.dir(project, slug).await?;
        let bytes = fs::read(dir.join("meta.json")).await?;
        let mut meta: Meta = serde_json::from_slice(&bytes)?;
        if meta.project.is_empty() {
            meta.project = project.to_string();
        }
        Ok(meta)
    }

    pub async fn get_content(&self, project: &str, slug: &str) -> Result<Vec<u8>, StoreError> {
        let dir = self.dir(project, slug).await?;
        Ok(fs::read(dir.join("content")).await?)
    }

    pub async fn delete(&self, project: &str, slug: &str) -> Result<(), StoreError> {
        let dir = self.dir(project, slug).await?;
        if !dir.exists() {
            return Err(StoreError::NotFound);
        }
        fs::remove_dir_all(&dir).await?;
        if let Some(parent) = dir.parent() {
            if parent != self.root {
                if let Ok(mut entries) = fs::read_dir(parent).await {
                    if entries.next_entry().await?.is_none() {
                        let _ = fs::remove_dir(parent).await;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<Meta>, StoreError> {
        let mut out = Vec::new();
        let mut entries = match fs::read_dir(&self.root).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let path = entry.path();
            let meta_path = path.join("meta.json");
            if tokio::fs::try_exists(&meta_path).await.unwrap_or(false) {
                if let Ok(bytes) = fs::read(&meta_path).await {
                    if let Ok(mut meta) = serde_json::from_slice::<Meta>(&bytes) {
                        if meta.project.is_empty() {
                            meta.project = DEFAULT_PROJECT.to_string();
                        }
                        out.push(meta);
                    }
                }
                continue;
            }
            let Some(project) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            if Self::validate_project(&project).is_err() {
                continue;
            }
            let mut kids = match fs::read_dir(&path).await {
                Ok(k) => k,
                Err(_) => continue,
            };
            while let Some(kid) = kids.next_entry().await? {
                if !kid.file_type().await?.is_dir() {
                    continue;
                }
                let kid_meta = kid.path().join("meta.json");
                let Ok(bytes) = fs::read(kid_meta).await else {
                    continue;
                };
                if let Ok(mut meta) = serde_json::from_slice::<Meta>(&bytes) {
                    meta.project = project.clone();
                    out.push(meta);
                }
            }
        }
        out.sort_by(|a, b| b.updated.cmp(&a.updated));
        Ok(out)
    }

    /// Directory of media files for a document, when it exists.
    pub async fn media_dir(&self, project: &str, slug: &str) -> Option<PathBuf> {
        let dir = self.dir(project, slug).await.ok()?.join("media");
        if tokio::fs::try_exists(&dir).await.unwrap_or(false) {
            Some(dir)
        } else {
            None
        }
    }

    pub async fn put_media(
        &self,
        project: &str,
        slug: &str,
        rel: &str,
        body: &[u8],
    ) -> Result<String, StoreError> {
        let rel = crate::media::normalize_media_rel(rel).ok_or(StoreError::BadSlug)?;
        let doc = self.require_doc(project, slug).await?;
        let (root, path) = media_file(&doc, &rel)?;
        fs::create_dir_all(&root).await?;
        ensure_media_parent(&root, &path).await?;
        let tmp = tmp_sibling(&path);
        fs::write(&tmp, body).await?;
        fs::rename(&tmp, &path).await?;
        Ok(rel)
    }

    pub async fn get_media(
        &self,
        project: &str,
        slug: &str,
        rel: &str,
    ) -> Result<(String, Vec<u8>), StoreError> {
        let rel = crate::media::normalize_media_rel(rel).ok_or(StoreError::BadSlug)?;
        let doc = self.require_doc(project, slug).await?;
        let (root, path) = media_file(&doc, &rel)?;
        let canon = contained_file(&root, &path).await?;
        Ok((rel, fs::read(canon).await?))
    }

    pub async fn delete_media(
        &self,
        project: &str,
        slug: &str,
        rel: &str,
    ) -> Result<(), StoreError> {
        let rel = crate::media::normalize_media_rel(rel).ok_or(StoreError::BadSlug)?;
        let doc = self.require_doc(project, slug).await?;
        let (_root, path) = media_file(&doc, &rel)?;
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(StoreError::NotFound),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn list_media(&self, project: &str, slug: &str) -> Result<Vec<String>, StoreError> {
        let doc = self.require_doc(project, slug).await?;
        let root = doc.join("media");
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = match fs::read_dir(&dir).await {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && dir == root => {
                    return Ok(out);
                }
                Err(e) => return Err(e.into()),
            };
            while let Some(entry) = entries.next_entry().await? {
                let file_type = entry.file_type().await?;
                if file_type.is_symlink() {
                    continue;
                }
                let path = entry.path();
                if file_type.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&root) else {
                    continue;
                };
                let rel = rel.to_string_lossy().replace('\\', "/");
                if let Some(rel) = crate::media::normalize_media_rel(&rel) {
                    out.push(rel);
                }
            }
        }
        out.sort();
        Ok(out)
    }

    pub async fn clear_media(&self, project: &str, slug: &str) -> Result<(), StoreError> {
        let doc = self.require_doc(project, slug).await?;
        let dir = doc.join("media");
        match fs::remove_dir_all(&dir).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn require_doc(&self, project: &str, slug: &str) -> Result<PathBuf, StoreError> {
        let dir = self.dir(project, slug).await?;
        if !tokio::fs::try_exists(dir.join("meta.json"))
            .await
            .unwrap_or(false)
        {
            return Err(StoreError::NotFound);
        }
        Ok(dir)
    }
}

fn media_file(doc_dir: &Path, rel: &str) -> Result<(PathBuf, PathBuf), StoreError> {
    let root = doc_dir.join("media");
    let mut path = root.clone();
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(StoreError::BadSlug);
        }
        path.push(seg);
    }
    Ok((root, path))
}

fn tmp_sibling(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Parent must stay inside the media directory after symlink resolution.
async fn ensure_media_parent(root: &Path, path: &Path) -> Result<(), StoreError> {
    let Some(parent) = path.parent() else {
        return Err(StoreError::BadSlug);
    };
    fs::create_dir_all(parent).await?;
    let canon_root = fs::canonicalize(root).await?;
    let canon_parent = fs::canonicalize(parent).await?;
    if !canon_parent.starts_with(&canon_root) {
        return Err(StoreError::BadSlug);
    }
    if let Ok(meta) = fs::symlink_metadata(path).await {
        if meta.file_type().is_symlink() {
            return Err(StoreError::BadSlug);
        }
    }
    Ok(())
}

async fn contained_file(root: &Path, path: &Path) -> Result<PathBuf, StoreError> {
    let canon_root = match fs::canonicalize(root).await {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(StoreError::NotFound),
        Err(e) => return Err(e.into()),
    };
    let canon = match fs::canonicalize(path).await {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(StoreError::NotFound),
        Err(e) => return Err(e.into()),
    };
    if canon.starts_with(&canon_root) {
        Ok(canon)
    } else {
        Err(StoreError::BadSlug)
    }
}

/// Strip Latin accents so a name can be a URL. `Đ`/`đ` do not decompose;
/// everything else Vietnamese uses is a base letter plus combining marks.
pub fn fold_latin(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.nfd() {
        match c {
            'Đ' | 'đ' => out.push('d'),
            '\u{0300}'..='\u{036F}' => {}
            c => out.push(c),
        }
    }
    out
}

/// `Tiếng Việt.md` → `tieng-viet.md`. None when no URL-safe characters remain.
pub fn ascii_slug(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let (stem, ext) = split_simple_ext(name);
    let folded = fold_latin(stem);
    let mut slug = String::new();
    let mut dash = false;
    for c in folded.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            dash = false;
        } else if !slug.is_empty() && !dash {
            slug.push('-');
            dash = true;
        }
    }
    let stem = slug.trim_matches('-');
    if stem.is_empty() {
        return None;
    }
    let ext = ext.map(|e| e.to_ascii_lowercase());
    let ext_len = ext.as_ref().map(|e| e.len() + 1).unwrap_or(0);
    if ext_len >= 128 {
        return None;
    }
    let stem = truncate_bytes(stem, 128 - ext_len).trim_matches('-');
    if stem.is_empty() {
        return None;
    }
    let full = match ext {
        Some(ext) => format!("{stem}.{ext}"),
        None => stem.to_string(),
    };
    Store::validate_slug(&full).ok().map(|_| full)
}

fn split_simple_ext(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && (1..=8).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            (stem, Some(ext))
        }
        _ => (name, None),
    }
}

fn truncate_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub fn guess_title(slug: &str, content_type: &str, body: &[u8]) -> String {
    if content_type.contains("markdown") || slug.ends_with(".md") {
        if let Ok(text) = std::str::from_utf8(body) {
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix('#') {
                    let title = rest.trim_start_matches('#').trim();
                    if !title.is_empty() {
                        return title.to_string();
                    }
                }
            }
        }
    }
    if content_type.contains("html") {
        if let Ok(text) = std::str::from_utf8(body) {
            if let Some(title) = html_title(text) {
                return title;
            }
        }
    }
    slug.to_string()
}

fn html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title>")? + 7;
    let end_rel = lower[start..].find("</title>")?;
    let title = html[start..start + end_rel].trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

/// Content-Type from a filename extension. Used by `gist put`.
pub fn content_type_from_filename(name: &str) -> String {
    from_extension(name).unwrap_or_else(|| "application/octet-stream".into())
}

/// Prefer a real HTTP Content-Type header; fall back to the name's extension.
/// `application/octet-stream` is treated as "unknown". Browsers often send
/// `text/plain` for `.md`, so that pair is upgraded to `text/markdown`.
pub fn guess_content_type(name: &str, header: Option<&str>) -> String {
    if let Some(ct) = header.and_then(from_header) {
        if ct.eq_ignore_ascii_case("text/plain") {
            if let Some(from_ext) = from_extension(name) {
                if from_ext == "text/markdown" {
                    return from_ext;
                }
            }
        }
        return ct;
    }
    content_type_from_filename(name)
}

fn from_extension(name: &str) -> Option<String> {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    if matches!(ext.as_str(), "md" | "markdown" | "mdown") {
        return Some("text/markdown".into());
    }
    mime_guess::from_ext(&ext)
        .first_raw()
        .filter(|ct| *ct != "application/octet-stream")
        .map(|ct| ct.to_string())
}

fn from_header(ct: &str) -> Option<String> {
    let ct = ct.split(';').next().unwrap_or(ct).trim();
    if ct.is_empty() || ct.eq_ignore_ascii_case("application/octet-stream") {
        None
    } else {
        Some(ct.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_rules() {
        assert!(Store::validate_slug("notes.md").is_ok());
        assert!(Store::validate_slug("a").is_ok());
        assert!(Store::validate_slug("../etc").is_err());
        assert!(Store::validate_slug(".hidden").is_err());
        assert!(Store::validate_slug("foo/bar").is_err());
        assert!(Store::validate_slug("").is_err());
        assert!(Store::validate_project("gist").is_ok());
        assert!(Store::validate_project("my-app").is_ok());
        assert!(Store::validate_slug("Tiếng Việt.md").is_err());
    }

    #[test]
    fn vietnamese_names_fold_into_url_slugs() {
        assert_eq!(
            Store::normalize_slug("Tiếng Việt.md").as_deref(),
            Some("tieng-viet.md")
        );
        assert_eq!(ascii_slug("Đường đi.md").as_deref(), Some("duong-di.md"));
        assert_eq!(ascii_slug("Ở nhà.md").as_deref(), Some("o-nha.md"));
        assert_eq!(
            Store::normalize_slug("Notes.md").as_deref(),
            Some("Notes.md")
        );
        assert_eq!(ascii_slug("!!!").as_deref(), None);
        assert_eq!(Store::normalize_slug("../etc"), None);
        assert_eq!(Store::normalize_slug("foo/bar.md"), None);
    }

    #[test]
    fn filename_maps_common_extensions() {
        assert_eq!(content_type_from_filename("notes.md"), "text/markdown");
        assert_eq!(content_type_from_filename("NOTES.MD"), "text/markdown");
        assert_eq!(
            content_type_from_filename("readme.markdown"),
            "text/markdown"
        );
        assert_eq!(
            content_type_from_filename("/tmp/docs/notes.md"),
            "text/markdown"
        );
        assert_eq!(content_type_from_filename("photo.png"), "image/png");
        assert_eq!(content_type_from_filename("report.pdf"), "application/pdf");
        assert_eq!(content_type_from_filename("page.html"), "text/html");
        assert_eq!(content_type_from_filename("data.json"), "application/json");
        assert_eq!(
            content_type_from_filename("no-ext"),
            "application/octet-stream"
        );
    }

    #[test]
    fn header_wins_over_extension() {
        assert_eq!(
            guess_content_type("notes.md", Some("application/json")),
            "application/json"
        );
        assert_eq!(
            guess_content_type("photo.png", Some("image/jpeg")),
            "image/jpeg"
        );
        assert_eq!(
            guess_content_type("notes", Some("text/markdown")),
            "text/markdown"
        );
        assert_eq!(
            guess_content_type("notes", Some("text/plain; charset=utf-8")),
            "text/plain"
        );
    }

    #[test]
    fn unknown_header_falls_back_to_filename() {
        assert_eq!(guess_content_type("notes.md", None), "text/markdown");
        assert_eq!(
            guess_content_type("notes.md", Some("application/octet-stream")),
            "text/markdown"
        );
        assert_eq!(
            guess_content_type("notes.md", Some("text/plain")),
            "text/markdown"
        );
        assert_eq!(
            guess_content_type("notes", Some("application/octet-stream")),
            "application/octet-stream"
        );
        assert_eq!(
            guess_content_type("notes", None),
            "application/octet-stream"
        );
    }

    #[tokio::test]
    async fn media_roundtrip_stays_inside_the_document() {
        let dir = std::env::temp_dir().join(format!("gist-store-{}", nanoid::nanoid!(6)));
        let store = Store::new(dir.clone());
        store
            .put(
                "inbox",
                "notes.md",
                "Notes".into(),
                "text/markdown".into(),
                b"# Hi",
            )
            .await
            .unwrap();
        let rel = store
            .put_media("inbox", "notes.md", "images/a.png", b"png-bytes")
            .await
            .unwrap();
        assert_eq!(rel, "images/a.png");
        let (got_rel, bytes) = store
            .get_media("inbox", "notes.md", "images/a.png")
            .await
            .unwrap();
        assert_eq!(got_rel, "images/a.png");
        assert_eq!(bytes, b"png-bytes");
        assert_eq!(
            store.list_media("inbox", "notes.md").await.unwrap(),
            vec!["images/a.png".to_string()]
        );
        assert!(store
            .put_media("inbox", "notes.md", "../secret.png", b"no")
            .await
            .is_err());
        assert!(store
            .put_media("inbox", "missing.md", "a.png", b"no")
            .await
            .is_err());
        store.clear_media("inbox", "notes.md").await.unwrap();
        assert!(store
            .list_media("inbox", "notes.md")
            .await
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
