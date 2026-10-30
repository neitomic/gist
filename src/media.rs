//! Files that belong to one document: images, audio, and video referenced by
//! a markdown or HTML page. Paths stay relative to that document. `..`,
//! absolute paths, and other file types are refused so a page cannot point
//! the server at something outside its own media directory.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const MAX_MEDIA_FILES: usize = 64;

const MAX_SEGMENTS: usize = 8;
const MAX_SEGMENT_LEN: usize = 180;
const MAX_REL_LEN: usize = 512;

pub struct UploadSource {
    pub body: Vec<u8>,
    pub type_name: String,
    pub default_slug: String,
    pub assets: Vec<(String, PathBuf)>,
    pub sync_media: bool,
}

pub fn is_page_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".md")
        || lower.ends_with(".markdown")
        || lower.ends_with(".mdown")
        || lower.ends_with(".html")
        || lower.ends_with(".htm")
}

/// Read a file or a directory and the media files that should travel with it.
/// A directory needs `index.html` / `index.md`, or exactly one page file.
pub fn collect_upload(path: &Path) -> Result<UploadSource, String> {
    if path.is_dir() {
        return collect_dir(path);
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("{}: file name is not valid UTF-8", path.display()))?;
    let body = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sync_media = is_page_name(name);
    let assets = if sync_media {
        let text = String::from_utf8_lossy(&body);
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        resolve_media(base, &referenced_media(&text))?
    } else {
        Vec::new()
    };
    Ok(UploadSource {
        body,
        type_name: name.to_string(),
        default_slug: name.to_string(),
        assets,
        sync_media,
    })
}

/// Relative paths in `text` that point at an allowed media file.
pub fn referenced_media(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    scan_markdown_dests(text, &mut found);
    scan_reference_defs(text, &mut found);
    scan_html_attrs(text, &mut found);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in found {
        let Some(rel) = normalize_media_rel(strip_query_frag(raw.trim())) else {
            continue;
        };
        if seen.insert(rel.clone()) {
            out.push(rel);
        }
    }
    out
}

/// Keep references that exist as files inside `base`. Symlinks that leave
/// `base` are dropped.
pub fn resolve_media(base: &Path, rels: &[String]) -> Result<Vec<(String, PathBuf)>, String> {
    let base = base
        .canonicalize()
        .map_err(|e| format!("{}: {e}", base.display()))?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for rel in rels {
        let Some(rel) = normalize_media_rel(rel) else {
            continue;
        };
        if !seen.insert(rel.clone()) {
            continue;
        }
        let path = join_rel(&base, &rel);
        let Ok(canon) = path.canonicalize() else {
            continue;
        };
        if !canon.starts_with(&base) || !canon.is_file() {
            continue;
        }
        out.push((rel, canon));
        if out.len() > MAX_MEDIA_FILES {
            return Err(format!(
                "too many media files ({}, max {MAX_MEDIA_FILES})",
                out.len()
            ));
        }
    }
    Ok(out)
}

/// Storage key for a media file. `None` when the path escapes, has no
/// allowed extension, or is not a relative path.
pub fn normalize_media_rel(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_start_matches('/');
    if raw.is_empty() {
        return None;
    }
    let decoded = percent_decode(raw)?;
    if decoded.contains(['?', '#', '\\', '\0']) {
        return None;
    }
    let mut parts = Vec::new();
    for seg in decoded.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." || seg.contains(':') || seg.chars().any(|c| c.is_control()) {
            return None;
        }
        if seg.len() > MAX_SEGMENT_LEN {
            return None;
        }
        parts.push(seg.to_string());
    }
    if parts.is_empty() || parts.len() > MAX_SEGMENTS {
        return None;
    }
    let rel = parts.join("/");
    if rel.len() > MAX_REL_LEN || !has_media_ext(&rel) {
        return None;
    }
    Some(rel)
}

pub fn encode_rel(rel: &str) -> String {
    rel.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// Rewrite a relative media URL onto `prefix` (`/d/{project}/{slug}/media`).
/// Absolute URLs, fragments, and non-media paths are left alone (`None`).
pub fn rewrite_media_ref(url: &str, prefix: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || prefix.is_empty() {
        return None;
    }
    let (without_frag, frag) = split_once(url, '#');
    let (path, query) = split_once(without_frag, '?');
    if !is_path_relative(path) {
        return None;
    }
    let rel = normalize_media_rel(path)?;
    let mut out = format!("{}/{}", prefix.trim_end_matches('/'), encode_rel(&rel));
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(frag) = frag {
        out.push('#');
        out.push_str(frag);
    }
    Some(out)
}

pub fn rewrite_srcset(value: &str, prefix: &str) -> String {
    value
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let mut bits = part.split_whitespace();
            let url = bits.next().unwrap_or("");
            let desc: Vec<&str> = bits.collect();
            let url = rewrite_media_ref(url, prefix).unwrap_or_else(|| url.to_string());
            if desc.is_empty() {
                Some(url)
            } else {
                Some(format!("{url} {}", desc.join(" ")))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn collect_dir(dir: &Path) -> Result<UploadSource, String> {
    let primary = find_primary(dir)?;
    let name = primary
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("{}: file name is not valid UTF-8", primary.display()))?;
    let body = std::fs::read(&primary).map_err(|e| format!("{}: {e}", primary.display()))?;
    let assets = walk_media(dir, &primary)?;
    Ok(UploadSource {
        body,
        type_name: name.to_string(),
        default_slug: directory_slug(dir, &primary),
        assets,
        sync_media: true,
    })
}

fn find_primary(dir: &Path) -> Result<PathBuf, String> {
    let mut index: Option<(u8, PathBuf)> = None;
    let mut pages = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let rank = match lower.as_str() {
            "index.html" => Some(0),
            "index.htm" => Some(1),
            "index.md" => Some(2),
            "index.markdown" => Some(3),
            "index.mdown" => Some(4),
            _ => None,
        };
        if let Some(rank) = rank {
            let replace = index.as_ref().is_none_or(|(prev, _)| rank < *prev);
            if replace {
                index = Some((rank, path));
            }
            continue;
        }
        if is_page_name(name) {
            pages.push(path);
        }
    }
    if let Some((_, path)) = index {
        return Ok(path);
    }
    if pages.len() == 1 {
        return Ok(pages.remove(0));
    }
    Err(format!(
        "{}: directory needs index.html or index.md, or a single markdown or html file",
        dir.display()
    ))
}

fn directory_slug(dir: &Path, primary: &Path) -> String {
    let file_name = primary
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("index.html");
    let lower = file_name.to_ascii_lowercase();
    let ext = file_name.rsplit_once('.').map(|(_, e)| e).unwrap_or("html");
    if lower.starts_with("index.") {
        let dir_name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("page");
        format!("{dir_name}.{ext}")
    } else {
        file_name.to_string()
    }
}

fn walk_media(dir: &Path, primary: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let root = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let primary = primary
        .canonicalize()
        .unwrap_or_else(|_| primary.to_path_buf());
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(current) = stack.pop() {
        let entries =
            std::fs::read_dir(&current).map_err(|e| format!("{}: {e}", current.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", current.display()))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|e| format!("{}: {e}", entry.path().display()))?;
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
            let Ok(canon) = path.canonicalize() else {
                continue;
            };
            if canon == primary || !canon.starts_with(&root) {
                continue;
            }
            let Ok(rel) = canon.strip_prefix(&root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let Some(rel) = normalize_media_rel(&rel) else {
                continue;
            };
            out.push((rel, canon));
            if out.len() > MAX_MEDIA_FILES {
                return Err(format!(
                    "too many media files ({}, max {MAX_MEDIA_FILES})",
                    out.len()
                ));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn join_rel(base: &Path, rel: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    for seg in rel.split('/') {
        path.push(seg);
    }
    path
}

fn has_media_ext(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    if ext.is_empty() || ext.len() > 5 {
        return false;
    }
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "avif"
            | "bmp"
            | "ico"
            | "svg"
            | "mp4"
            | "webm"
            | "mp3"
            | "wav"
            | "ogg"
            | "m4a"
    )
}

fn is_path_relative(path: &str) -> bool {
    let path = path.trim();
    if path.is_empty() || path.starts_with('#') || path.starts_with('/') || path.starts_with("//") {
        return false;
    }
    // `mailto:`, `https://`, `data:` — a scheme before the first slash.
    if let Some(head) = path.split(['/', '?', '#']).next() {
        if let Some(colon) = head.find(':') {
            let scheme = &head[..colon];
            if !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
            {
                return false;
            }
        }
    }
    true
}

fn strip_query_frag(url: &str) -> &str {
    let (path, _) = split_once(url, '#');
    split_once(path, '?').0
}

fn split_once(s: &str, sep: char) -> (&str, Option<&str>) {
    match s.split_once(sep) {
        Some((head, tail)) => (head, Some(tail)),
        None => (s, None),
    }
}

fn scan_markdown_dests(text: &str, out: &mut Vec<String>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            i += 2;
            if i < bytes.len() && bytes[i] == b'<' {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'>' && bytes[i] != b'\n' {
                    i += 1;
                }
                push_url(&text[start..i], out);
            } else {
                let start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b')' {
                    i += 1;
                }
                push_url(&text[start..i], out);
            }
        } else {
            i += 1;
        }
    }
}

fn scan_reference_defs(text: &str, out: &mut Vec<String>) {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('[') else {
            continue;
        };
        let Some(idx) = rest.find("]:") else {
            continue;
        };
        let after = rest[idx + 2..].trim();
        if after.is_empty() {
            continue;
        }
        let url = if let Some(rest) = after.strip_prefix('<') {
            rest.split('>').next().unwrap_or("")
        } else {
            after.split_whitespace().next().unwrap_or("")
        };
        push_url(url, out);
    }
}

fn scan_html_attrs(text: &str, out: &mut Vec<String>) {
    let lower = text.to_ascii_lowercase();
    for key in ["src=", "href=", "poster="] {
        let mut from = 0;
        while let Some(at) = find_attr(&lower, from, key) {
            if let Some((value, next)) = attr_value(text, at + key.len()) {
                push_url(value, out);
                from = next;
            } else {
                from = at + key.len();
            }
        }
    }
    let mut from = 0;
    while let Some(at) = find_attr(&lower, from, "srcset=") {
        if let Some((value, next)) = attr_value(text, at + "srcset=".len()) {
            for part in value.split(',') {
                if let Some(url) = part.split_whitespace().next() {
                    push_url(url, out);
                }
            }
            from = next;
        } else {
            from = at + "srcset=".len();
        }
    }
}

fn find_attr(lower: &str, from: usize, key: &str) -> Option<usize> {
    let mut from = from;
    while let Some(rel) = lower[from..].find(key) {
        let at = from + rel;
        let boundary = at == 0
            || !matches!(
                lower.as_bytes()[at - 1],
                b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b':'
            );
        if boundary {
            return Some(at);
        }
        from = at + key.len();
    }
    None
}

fn attr_value(text: &str, mut i: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    if i >= bytes.len() {
        return None;
    }
    let quote = bytes[i];
    if quote == b'"' || quote == b'\'' {
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        let end = i;
        if i < bytes.len() {
            i += 1;
        }
        return Some((&text[start..end], i));
    }
    None
}

fn push_url(url: &str, out: &mut Vec<String>) {
    let url = url.trim();
    if !url.is_empty() {
        out.push(url.to_string());
    }
}

fn encode_segment(seg: &str) -> String {
    let mut out = String::new();
    for &b in seg.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_relative_media_and_rejects_escapes() {
        assert_eq!(
            normalize_media_rel("./images/hero.PNG").as_deref(),
            Some("images/hero.PNG")
        );
        assert_eq!(
            normalize_media_rel("shots/a.png").as_deref(),
            Some("shots/a.png")
        );
        assert_eq!(normalize_media_rel("../secret.png"), None);
        assert_eq!(normalize_media_rel("foo/../../x.png"), None);
        assert_eq!(normalize_media_rel("%2e%2e/%2e%2e/x.png"), None);
        assert_eq!(normalize_media_rel("/etc/passwd"), None);
        assert_eq!(normalize_media_rel("https://example.com/a.png"), None);
        assert_eq!(normalize_media_rel("notes.md"), None);
        assert_eq!(normalize_media_rel("page.html"), None);
        assert_eq!(normalize_media_rel("script.js"), None);
        assert_eq!(
            normalize_media_rel("images/my%20shot.png").as_deref(),
            Some("images/my shot.png")
        );
    }

    #[test]
    fn rewrites_only_relative_media() {
        let prefix = "/d/inbox/notes.md/media";
        assert_eq!(
            rewrite_media_ref("images/hero.png", prefix).as_deref(),
            Some("/d/inbox/notes.md/media/images/hero.png")
        );
        assert_eq!(
            rewrite_media_ref("./shot.png", prefix).as_deref(),
            Some("/d/inbox/notes.md/media/shot.png")
        );
        assert_eq!(rewrite_media_ref("https://example.com/a.png", prefix), None);
        assert_eq!(rewrite_media_ref("other.md", prefix), None);
        assert_eq!(rewrite_media_ref("#section", prefix), None);
        assert_eq!(rewrite_media_ref("/already/absolute.png", prefix), None);
        assert_eq!(
            rewrite_srcset("shot.png 1x, https://cdn.example/a.png 2x", prefix),
            "/d/inbox/notes.md/media/shot.png 1x, https://cdn.example/a.png 2x"
        );
    }

    #[test]
    fn references_in_markdown_and_html() {
        let text = r#"
![Hero](images/hero.png)
[design](design.md)
[remote](https://example.com/a.png)

<img src="shots/b.jpg" alt="b">
<video src="clip.mp4" poster="shots/poster.png"></video>
<img srcset="shots/c.png 1x, shots/c@2.png 2x">
<a href="other.html">page</a>

[id]: images/ref.webp
"#;
        let refs = referenced_media(text);
        assert!(refs.contains(&"images/hero.png".to_string()), "{refs:?}");
        assert!(refs.contains(&"shots/b.jpg".to_string()), "{refs:?}");
        assert!(refs.contains(&"clip.mp4".to_string()), "{refs:?}");
        assert!(refs.contains(&"shots/poster.png".to_string()), "{refs:?}");
        assert!(refs.contains(&"shots/c.png".to_string()), "{refs:?}");
        assert!(refs.contains(&"shots/c@2.png".to_string()), "{refs:?}");
        assert!(refs.contains(&"images/ref.webp".to_string()), "{refs:?}");
        assert!(!refs
            .iter()
            .any(|r| r.ends_with(".md") || r.ends_with(".html")));
        assert!(!refs.iter().any(|r| r.contains("example.com")));
    }

    #[test]
    fn collect_file_includes_a_sibling_image() {
        let dir = std::env::temp_dir().join(format!("gist-media-{}", nanoid::nanoid!(6)));
        std::fs::create_dir_all(dir.join("images")).unwrap();
        let page = dir.join("notes.md");
        std::fs::write(&page, "![x](images/a.png)\n\n[nope](missing.png)\n").unwrap();
        std::fs::write(dir.join("images/a.png"), b"png").unwrap();
        let got = collect_upload(&page).unwrap();
        assert!(got.sync_media);
        assert_eq!(got.assets.len(), 1);
        assert_eq!(got.assets[0].0, "images/a.png");
        assert!(!got.assets.iter().any(|(rel, _)| rel == "missing.png"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn collect_directory_uses_index_and_nested_media() {
        let dir = std::env::temp_dir().join(format!("gist-media-{}", nanoid::nanoid!(6)));
        let root = dir.join("Report");
        std::fs::create_dir_all(root.join("img")).unwrap();
        std::fs::write(root.join("index.html"), "<img src=\"img/a.png\">").unwrap();
        std::fs::write(root.join("img/a.png"), b"png").unwrap();
        std::fs::write(root.join("notes.txt"), "ignore").unwrap();
        let got = collect_upload(&root).unwrap();
        assert_eq!(got.type_name, "index.html");
        assert_eq!(got.default_slug, "Report.html");
        assert_eq!(got.assets.len(), 1);
        assert_eq!(got.assets[0].0, "img/a.png");
        let _ = std::fs::remove_dir_all(dir);
    }
}
