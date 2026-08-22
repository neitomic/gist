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
            View::Html(Rendered::simple(sanitize(&text)))
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
        html: sanitize(&html_out),
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
        assert!(html.html.contains("<h1>Ok</h1>"));
        assert!(!html.html.contains("onerror"));
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
