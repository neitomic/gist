use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use time::OffsetDateTime;
use tokio::fs;

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

pub fn guess_content_type(slug: &str, header: Option<&str>) -> String {
    if let Some(ct) = header {
        let ct = ct.split(';').next().unwrap_or(ct).trim();
        if !ct.is_empty() && ct != "application/octet-stream" && ct != "text/plain" {
            return ct.to_string();
        }
        if ct == "text/plain" {
            if slug.ends_with(".md") || slug.ends_with(".markdown") {
                return "text/markdown".into();
            }
            return ct.to_string();
        }
    }
    mime_guess::from_path(slug)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_string()
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
    }
}
