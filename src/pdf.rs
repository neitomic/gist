use crate::render::Kind;
use markdown2pdf::config::ConfigSource;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub fn filename(slug: &str) -> String {
    let slug = slug.replace(['"', '/', '\\'], "");
    let lower = slug.to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        slug
    } else if let Some((stem, _)) = slug.rsplit_once('.') {
        if stem.is_empty() {
            format!("{slug}.pdf")
        } else {
            format!("{stem}.pdf")
        }
    } else {
        format!("{slug}.pdf")
    }
}

pub fn render(kind: Kind, title: &str, slug: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    match kind {
        Kind::Pdf => Ok(body.to_vec()),
        Kind::File => Err("this file type cannot be exported as PDF".into()),
        Kind::Image => render_image(title, slug, body),
        Kind::Markdown | Kind::Html | Kind::Text => {
            let markdown = to_markdown(kind, title, body);
            bytes_from_markdown(&markdown, title)
        }
    }
}

fn bytes_from_markdown(markdown: &str, title: &str) -> Result<Vec<u8>, String> {
    let cfg = config_toml(title);
    markdown2pdf::parse_into_bytes(
        markdown.to_string(),
        ConfigSource::Embedded(&cfg),
        None,
    )
    .map_err(|e| e.to_string())
}

fn config_toml(title: &str) -> String {
    format!(
        r#"
theme = "github"

[metadata]
title = {title}
creator = "gist"

[page]
size = "A4"
orientation = "portrait"
margins = {{ top = 20.0, right = 18.0, bottom = 20.0, left = 18.0 }}

[toc]
enabled = false

[security]
allow_remote_images = false
"#,
        title = toml_string(title),
    )
}

fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn to_markdown(kind: Kind, title: &str, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    match kind {
        Kind::Markdown => text.into_owned(),
        Kind::Html => html_to_markdown(title, &text),
        Kind::Text => format!("# {}\n\n```\n{}\n```\n", heading_text(title), text),
        _ => format!("# {}\n", heading_text(title)),
    }
}

fn heading_text(s: &str) -> String {
    s.replace(['\n', '\r', '#'], " ").trim().to_string()
}

fn html_to_markdown(title: &str, html: &str) -> String {
    let mut out = String::new();
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    let mut saw_h1 = false;
    while let Some((start, level)) = find_heading_open(&lower, i) {
        out.push_str(&html_chunk_to_md(&html[i..start]));
        let Some(open_end) = html[start..].find('>').map(|n| start + n) else {
            break;
        };
        let close_pat = format!("</h{level}");
        let search_from = open_end + 1;
        let Some(close_rel) = lower[search_from..].find(&close_pat) else {
            break;
        };
        let close_start = search_from + close_rel;
        let Some(close_end_rel) = html[close_start..].find('>') else {
            break;
        };
        let close_end = close_start + close_end_rel + 1;
        let inner = strip_tags(&html[open_end + 1..close_start]);
        let text = decode_entities(&inner);
        let text = heading_text(text.trim());
        if !text.is_empty() {
            if level == 1 {
                saw_h1 = true;
            }
            out.push_str("\n\n");
            out.push_str(&"#".repeat(level as usize));
            out.push(' ');
            out.push_str(&text);
            out.push_str("\n\n");
        }
        i = close_end;
    }
    out.push_str(&html_chunk_to_md(&html[i..]));
    let out = out.trim().to_string();
    if saw_h1 || out.starts_with('#') {
        out
    } else if out.is_empty() {
        format!("# {}\n", heading_text(title))
    } else {
        format!("# {}\n\n{}\n", heading_text(title), out)
    }
}

fn html_chunk_to_md(s: &str) -> String {
    let mut t = s.replace("<br>", "\n");
    t = t.replace("<br/>", "\n");
    t = t.replace("<br />", "\n");
    t = t.replace("<BR>", "\n");
    t = t.replace("<p>", "\n\n");
    t = t.replace("</p>", "\n");
    t = t.replace("<P>", "\n\n");
    t = t.replace("</P>", "\n");
    t = t.replace("<li>", "\n- ");
    t = t.replace("<LI>", "\n- ");
    t = t.replace("</li>", "");
    t = t.replace("<pre>", "\n```\n");
    t = t.replace("</pre>", "\n```\n");
    t = t.replace("<PRE>", "\n```\n");
    t = t.replace("</PRE>", "\n```\n");
    decode_entities(&strip_tags(&t))
}

fn find_heading_open(lower: &str, from: usize) -> Option<(usize, u8)> {
    let bytes = lower.as_bytes();
    let mut i = from;
    while i + 3 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'h' {
            let n = bytes[i + 2];
            if matches!(n, b'1' | b'2' | b'3' | b'4' | b'5' | b'6') {
                let next = bytes[i + 3];
                if next == b'>' || next.is_ascii_whitespace() {
                    return Some((i, n - b'0'));
                }
            }
        }
        i += 1;
    }
    None
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn render_image(title: &str, slug: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    let ext = image_ext(slug);
    let dir: PathBuf = std::env::temp_dir().join(format!("gist-pdf-{}", nanoid::nanoid!(8)));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let img = dir.join(format!("image{ext}"));
    let result = (|| {
        let mut f = fs::File::create(&img).map_err(|e| e.to_string())?;
        f.write_all(body).map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
        let md = format!(
            "# {}\n\n![]({})\n",
            heading_text(title),
            img.display()
        );
        bytes_from_markdown(&md, title)
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

fn image_ext(slug: &str) -> &'static str {
    match slug.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) {
        Some(ref e) if e == "png" => ".png",
        Some(ref e) if e == "jpg" || e == "jpeg" => ".jpg",
        Some(ref e) if e == "gif" => ".gif",
        Some(ref e) if e == "webp" => ".webp",
        _ => ".png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_pdf_is_document_with_outline() {
        let md = "# Design\n\nIntro paragraph.\n\n## Plan\n\nMore text.\n\n### Risks\n\nNone.\n";
        let pdf = render(Kind::Markdown, "Design", "design.md", md.as_bytes()).expect("pdf");
        assert!(pdf.starts_with(b"%PDF"), "not a PDF");
        let plain = inflate_pdf_streams(&pdf);
        assert!(
            plain.contains("/Outlines"),
            "expected a native PDF outline"
        );
        assert!(plain.contains("/Title"));
        assert!(!plain.to_ascii_lowercase().contains("logout"));
    }

    fn inflate_pdf_streams(bytes: &[u8]) -> String {
        use flate2::read::ZlibDecoder;
        use std::io::Read;
        let mut plain = bytes.to_vec();
        let mut i = 0;
        while i + 6 < bytes.len() {
            let Some(rel) = bytes[i..].windows(6).position(|w| w == b"stream") else {
                break;
            };
            let mut start = i + rel + 6;
            if bytes.get(start) == Some(&b'\r') {
                start += 1;
            }
            if bytes.get(start) == Some(&b'\n') {
                start += 1;
            }
            let Some(end_rel) = bytes[start..].windows(9).position(|w| w == b"endstream") else {
                break;
            };
            let mut blob = &bytes[start..start + end_rel];
            blob = blob.strip_suffix(b"\r\n").or_else(|| blob.strip_suffix(b"\n")).unwrap_or(blob);
            let mut dec = ZlibDecoder::new(blob);
            let mut out = Vec::new();
            if dec.read_to_end(&mut out).is_ok() {
                plain.extend_from_slice(&out);
            }
            i = start + end_rel + 9;
        }
        String::from_utf8_lossy(&plain).into_owned()
    }

    #[test]
    fn html_headings_become_markdown() {
        let md = html_to_markdown(
            "Fallback",
            "<h1>Cutover</h1><p>Hello</p><h2>Plan</h2><p>Do it.</p>",
        );
        assert!(md.contains("# Cutover"));
        assert!(md.contains("## Plan"));
        assert!(md.contains("Hello"));
        assert!(!md.contains("<h1>"));
    }

    #[test]
    fn filename_swaps_extension() {
        assert_eq!(filename("notes.md"), "notes.pdf");
        assert_eq!(filename("report.pdf"), "report.pdf");
        assert_eq!(filename("plain"), "plain.pdf");
    }
}
