use std::collections::HashMap;

use pulldown_cmark::{
    html, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

use crate::highlight;

pub enum View {
    Html(Rendered),
    Binary,
}

#[derive(Clone, Debug, Default)]
pub struct Rendered {
    pub html: String,
    pub toc: Vec<TocItem>,
    pub has_mermaid: bool,
    pub has_code: bool,
}

impl Rendered {
    pub fn simple(html: String) -> Self {
        Self {
            html,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct TocItem {
    pub level: u8,
    pub id: String,
    pub text: String,
}

pub fn view_kind(content_type: &str, slug: &str) -> Kind {
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("markdown") || slug.ends_with(".md") || slug.ends_with(".markdown") {
        return Kind::Markdown;
    }
    if ct.contains("html") || slug.ends_with(".html") || slug.ends_with(".htm") {
        return Kind::Html;
    }
    if ct.starts_with("text/")
        || ct.contains("json")
        || ct.contains("xml")
        || ct.contains("yaml")
        || ct.contains("csv")
        || matches!(
            ext(slug),
            "txt"
                | "json"
                | "xml"
                | "yml"
                | "yaml"
                | "csv"
                | "toml"
                | "rs"
                | "py"
                | "ts"
                | "js"
                | "sh"
                | "log"
        )
    {
        return Kind::Text;
    }
    if ct.starts_with("image/") {
        return Kind::Image;
    }
    if ct == "application/pdf" || slug.ends_with(".pdf") {
        return Kind::Pdf;
    }
    Kind::File
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Markdown,
    Html,
    Text,
    Image,
    Pdf,
    File,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Markdown => "Markdown",
            Kind::Html => "HTML",
            Kind::Text => "Text",
            Kind::Image => "Image",
            Kind::Pdf => "PDF",
            Kind::File => "File",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Kind::Markdown => "MD",
            Kind::Html => "HTML",
            Kind::Text => "TXT",
            Kind::Image => "IMG",
            Kind::Pdf => "PDF",
            Kind::File => "FILE",
        }
    }

    pub fn filter_key(self) -> &'static str {
        match self {
            Kind::Markdown => "md",
            Kind::Html => "html",
            Kind::Text => "text",
            Kind::Image => "image",
            Kind::Pdf => "pdf",
            Kind::File => "file",
        }
    }

    pub fn from_filter(s: &str) -> Option<Self> {
        match s {
            "md" | "markdown" => Some(Kind::Markdown),
            "html" => Some(Kind::Html),
            "text" | "txt" => Some(Kind::Text),
            "image" | "img" => Some(Kind::Image),
            "pdf" => Some(Kind::Pdf),
            "file" => Some(Kind::File),
            _ => None,
        }
    }

    pub const FILTERS: [Kind; 6] = [
        Kind::Markdown,
        Kind::Html,
        Kind::Text,
        Kind::Image,
        Kind::Pdf,
        Kind::File,
    ];
}

pub fn render_body(kind: Kind, body: &[u8]) -> View {
    match kind {
        Kind::Markdown => View::Html(render_markdown(&String::from_utf8_lossy(body))),
        Kind::Html => {
            let text = String::from_utf8_lossy(body);
            let html = wrap_tables(sanitize(&text));
            let (html, toc) = extract_html_toc(html);
            View::Html(Rendered {
                html,
                toc,
                ..Rendered::default()
            })
        }
        Kind::Text => {
            let text = esc(&String::from_utf8_lossy(body));
            View::Html(Rendered::simple(format!(
                "<pre class=\"plain\">{text}</pre>"
            )))
        }
        Kind::Image | Kind::Pdf | Kind::File => View::Binary,
    }
}

fn md_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_DEFINITION_LIST);
    options
}

fn render_markdown(text: &str) -> Rendered {
    let events: Vec<Event> = Parser::new_ext(text, md_options()).collect();
    let (events, toc, has_mermaid, has_code) = rewrite(events);
    let mut html_out = String::new();
    html::push_html(&mut html_out, events.into_iter());
    Rendered {
        html: wrap_tables(sanitize(&html_out)),
        toc,
        has_mermaid,
        has_code,
    }
}

fn rewrite<'a>(mut events: Vec<Event<'a>>) -> (Vec<Event<'a>>, Vec<TocItem>, bool, bool) {
    let mut toc = Vec::new();
    let mut used_ids: HashMap<String, u32> = HashMap::new();
    let mut has_mermaid = false;
    let mut has_code = false;
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }) => {
                let level = *level;
                let mut id = id.clone();
                let classes = classes.clone();
                let attrs = attrs.clone();
                let mut text = String::new();
                let mut j = i + 1;
                while j < events.len() {
                    match &events[j] {
                        Event::End(TagEnd::Heading(_)) => break,
                        Event::Text(t) | Event::Code(t) => text.push_str(t),
                        _ => {}
                    }
                    j += 1;
                }
                let raw_id = id
                    .take()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| slugify(&text));
                let id_str = unique_id(&mut used_ids, raw_id);
                if !text.trim().is_empty() {
                    toc.push(TocItem {
                        level: heading_level(level),
                        id: id_str.clone(),
                        text: text.trim().to_string(),
                    });
                }
                events[i] = Event::Start(Tag::Heading {
                    level,
                    id: Some(CowStr::from(id_str)),
                    classes,
                    attrs,
                });
                i = j.saturating_add(1);
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let info = match kind {
                    CodeBlockKind::Fenced(info) => info.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                let mut code = String::new();
                let mut j = i + 1;
                while j < events.len() {
                    match &events[j] {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(t) => code.push_str(t),
                        _ => {}
                    }
                    j += 1;
                }
                let lang = fence_lang(&info);
                let html = if is_mermaid(&lang) {
                    has_mermaid = true;
                    mermaid_html(&code)
                } else {
                    has_code = true;
                    highlight::code_html(&lang, &code)
                };
                events.splice(i..=j, std::iter::once(Event::Html(html.into())));
                i += 1;
            }
            _ => i += 1,
        }
    }
    (events, toc, has_mermaid, has_code)
}

fn fence_lang(info: &str) -> String {
    info.split(|c: char| c == ',' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn is_mermaid(lang: &str) -> bool {
    matches!(lang, "mermaid" | "mmd")
}

fn mermaid_html(code: &str) -> String {
    format!(
        "<div class=\"mermaid-wrap\"><pre class=\"mermaid\">{}</pre></div>\n",
        esc(code.trim())
    )
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if c.is_alphanumeric() {
            for x in c.to_lowercase() {
                out.push(x);
            }
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "section".into()
    } else {
        out
    }
}

fn unique_id(used: &mut HashMap<String, u32>, id: String) -> String {
    let n = used.entry(id.clone()).or_insert(0);
    *n += 1;
    if *n == 1 {
        id
    } else {
        format!("{id}-{}", *n - 1)
    }
}

fn sanitize(html: &str) -> String {
    ammonia::Builder::default()
        .add_tags(["div", "input", "section", "aside", "nav"])
        .add_generic_attributes(["class", "id"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("th", ["style"])
        .add_tag_attributes("td", ["style"])
        .filter_style_properties(["text-align"].into_iter().collect())
        .id_prefix(Some(""))
        .clean(html)
        .to_string()
}

fn extract_html_toc(html: String) -> (String, Vec<TocItem>) {
    let mut toc = Vec::new();
    let mut used_ids: HashMap<String, u32> = HashMap::new();
    let mut out = String::with_capacity(html.len() + 64);
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    while let Some((start, level)) = find_heading_open(&lower, i) {
        out.push_str(&html[i..start]);
        let Some(open_end) = html[start..].find('>').map(|n| start + n) else {
            out.push_str(&html[start..]);
            return (out, toc);
        };
        let close_pat = format!("</h{level}");
        let search_from = open_end + 1;
        let Some(close_rel) = lower[search_from..].find(&close_pat) else {
            out.push_str(&html[start..]);
            return (out, toc);
        };
        let close_start = search_from + close_rel;
        let Some(close_end_rel) = html[close_start..].find('>') else {
            out.push_str(&html[start..]);
            return (out, toc);
        };
        let close_end = close_start + close_end_rel + 1;
        let open_tag = &html[start..=open_end];
        let inner = &html[open_end + 1..close_start];
        let text = decode_entities(&strip_tags(inner));
        let text = text.trim();
        let mut open_out = open_tag.to_string();
        if !text.is_empty() {
            let id_str = if let Some(existing) = attr_id(open_tag) {
                let n = used_ids.entry(existing.clone()).or_insert(0);
                *n += 1;
                existing
            } else {
                let id_str = unique_id(&mut used_ids, slugify(text));
                open_out = format!(
                    "{} id=\"{}\">",
                    open_tag[..open_tag.len() - 1].trim_end(),
                    esc(&id_str)
                );
                id_str
            };
            toc.push(TocItem {
                level,
                id: id_str,
                text: text.to_string(),
            });
        }
        out.push_str(&open_out);
        out.push_str(inner);
        out.push_str(&html[close_start..close_end]);
        i = close_end;
    }
    if i < html.len() {
        out.push_str(&html[i..]);
    }
    (out, toc)
}

fn find_heading_open(lower: &str, from: usize) -> Option<(usize, u8)> {
    let bytes = lower.as_bytes();
    let mut i = from;
    while i + 3 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'h' {
            let n = bytes[i + 2];
            if matches!(n, b'1' | b'2' | b'3' | b'4') {
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

fn attr_id(open_tag: &str) -> Option<String> {
    let lower = open_tag.to_ascii_lowercase();
    let i = lower.find("id=")?;
    if i > 0 {
        let before = lower.as_bytes()[i - 1];
        if before.is_ascii_alphanumeric() || before == b'-' || before == b'_' {
            return None;
        }
    }
    let rest = open_tag[i + 3..].trim_start();
    let id = match rest.as_bytes().first() {
        Some(q @ (b'"' | b'\'')) => {
            let q = *q as char;
            let rest = &rest[1..];
            let end = rest.find(q)?;
            rest[..end].trim()
        }
        Some(_) => {
            let end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '>')
                .unwrap_or(rest.len());
            rest[..end].trim()
        }
        None => return None,
    };
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
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
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = if rest.starts_with("&amp;") {
            rest = &rest[5..];
            Some('&')
        } else if rest.starts_with("&lt;") {
            rest = &rest[4..];
            Some('<')
        } else if rest.starts_with("&gt;") {
            rest = &rest[4..];
            Some('>')
        } else if rest.starts_with("&quot;") {
            rest = &rest[6..];
            Some('"')
        } else if rest.starts_with("&#39;") || rest.starts_with("&apos;") {
            rest = if rest.starts_with("&#39;") {
                &rest[5..]
            } else {
                &rest[6..]
            };
            Some('\'')
        } else if rest.starts_with("&nbsp;") {
            rest = &rest[6..];
            Some(' ')
        } else {
            None
        };
        match decoded {
            Some(c) => out.push(c),
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn wrap_tables(html: String) -> String {
    if !html.contains("<table") {
        return html;
    }
    html.replace("<table", "<div class=\"table-wrap\"><table")
        .replace("</table>", "</table></div>")
}

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn ext(slug: &str) -> &str {
    slug.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_strips_script() {
        let View::Html(html) = render_body(Kind::Markdown, b"# Hi\n\n<script>alert(1)</script>")
        else {
            panic!("expected html");
        };
        assert!(html.html.contains("<h1"));
        assert!(html.html.contains(">Hi</h1>"));
        assert!(html.html.contains("id=\"hi\""));
        assert!(!html.html.to_ascii_lowercase().contains("<script"));
        assert_eq!(html.toc.len(), 1);
        assert_eq!(html.toc[0].id, "hi");
    }

    #[test]
    fn html_is_sanitized() {
        let View::Html(html) = render_body(Kind::Html, b"<h1>Ok</h1><img src=x onerror=alert(1)>")
        else {
            panic!("expected html");
        };
        assert!(html.html.contains("<h1"));
        assert!(html.html.contains("Ok</h1>"));
        assert!(!html.html.contains("onerror"));
    }

    #[test]
    fn html_headings_become_sidebar_toc() {
        let src = r##"
<nav class="toc"><ol><li><a href="#s1">Problem</a></li></ol></nav>
<h1>WFO Cutover</h1>
<section id="s1"><h2>Problem</h2></section>
<h2 id="custom">Vault topology &amp; naming</h2>
<h3>Details</h3>
"##;
        let View::Html(out) = render_body(Kind::Html, src.as_bytes()) else {
            panic!("expected html");
        };
        assert!(
            out.html.contains("id=\"problem\"") || out.html.contains("id=\"Problem\""),
            "expected generated heading id: {}",
            out.html
        );
        let texts: Vec<&str> = out.toc.iter().map(|t| t.text.as_str()).collect();
        assert!(texts.contains(&"WFO Cutover"), "{texts:?}");
        assert!(texts.contains(&"Problem"), "{texts:?}");
        assert!(texts.contains(&"Vault topology & naming"), "{texts:?}");
        assert!(texts.contains(&"Details"), "{texts:?}");
        assert_eq!(
            out.toc
                .iter()
                .find(|t| t.text == "Vault topology & naming")
                .map(|t| t.id.as_str()),
            Some("custom")
        );
        assert!(out.toc.iter().any(|t| t.level == 2 && t.id == "problem"));
    }

    #[test]
    fn highlights_rust_and_keeps_mermaid() {
        let md = r#"
## Code

```rust
fn main() { println!("hi"); }
```

## Diagram

```mermaid
graph TD
  A --> B
```
"#;
        let View::Html(out) = render_body(Kind::Markdown, md.as_bytes()) else {
            panic!("expected html");
        };
        assert!(out.has_code);
        assert!(out.has_mermaid);
        assert!(out.html.contains("syn-code"));
        assert!(out.html.contains("class=\"mermaid\""));
        assert!(out.html.contains("A --&gt; B") || out.html.contains("A --> B"));
        assert_eq!(
            out.toc.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["code", "diagram"]
        );
    }

    #[test]
    fn tables_are_wrapped_for_mobile_scroll() {
        let md = b"| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let View::Html(out) = render_body(Kind::Markdown, md) else {
            panic!("expected html");
        };
        assert!(out.html.contains("class=\"table-wrap\""));
        assert!(out.html.contains("<table"));
        assert!(out.html.contains("</table></div>") || out.html.contains("</table>\n</div>"));
    }

    #[test]
    fn alerts_and_task_lists() {
        let md = b"> [!NOTE]\n> Hello\n\n- [x] done\n- [ ] todo\n";
        let View::Html(out) = render_body(Kind::Markdown, md) else {
            panic!("expected html");
        };
        assert!(
            out.html.contains("markdown-alert-note"),
            "missing alert class: {}",
            out.html
        );
        assert!(
            out.html.contains("checkbox"),
            "missing checkbox: {}",
            out.html
        );
    }

    #[test]
    fn duplicate_headings_get_unique_ids() {
        let View::Html(out) = render_body(Kind::Markdown, b"# Same\n\n# Same\n") else {
            panic!("expected html");
        };
        assert_eq!(out.toc[0].id, "same");
        assert_eq!(out.toc[1].id, "same-1");
    }
}
