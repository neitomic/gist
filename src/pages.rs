use crate::highlight::{self, CODE_THEMES};
use crate::render::{esc, view_kind, Kind, Rendered, TocItem};
use crate::store::Meta;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::OffsetDateTime;

const CSS: &str = r#"
:root {
  --bg: #f4efe6;
  --ink: #1c1916;
  --muted: #6b635b;
  --line: #ddd4c6;
  --paper: #fffdf8;
  --accent: #8a3b12;
  --chip: #ece4d6;
  color-scheme: light dark;
}
html[data-theme="light"] { color-scheme: light; }
html[data-theme="dark"] {
  color-scheme: dark;
  --bg: #141210;
  --ink: #f3ece3;
  --muted: #a3988c;
  --line: #3a342d;
  --paper: #1c1916;
  --accent: #e08a55;
  --chip: #2a2520;
}
@media (prefers-color-scheme: dark) {
  html[data-theme="auto"] {
    --bg: #141210;
    --ink: #f3ece3;
    --muted: #a3988c;
    --line: #3a342d;
    --paper: #1c1916;
    --accent: #e08a55;
    --chip: #2a2520;
  }
}
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; overflow-x: clip; }
body {
  font: 16px/1.5 ui-sans-serif, system-ui, -apple-system, sans-serif;
  background: var(--bg);
  color: var(--ink);
}
a { color: var(--accent); }
header.app, footer.app, main { width: min(840px, calc(100% - 32px)); margin: 0 auto; min-width: 0; }
body.doc-page header.app, body.doc-page footer.app, body.doc-page main {
  width: min(1180px, calc(100% - 32px));
}
header.app {
  display: flex; align-items: center; justify-content: space-between;
  padding: 28px 0 16px; border-bottom: 1px solid var(--line); margin-bottom: 24px;
  gap: 16px;
}
header.app h1 { font-size: 1.05rem; font-weight: 650; letter-spacing: 0.02em; margin: 0; }
header.app h1 a { color: inherit; text-decoration: none; }
header.app .header-end { display: flex; align-items: center; gap: 16px; flex-wrap: wrap; }
header.app nav { display: flex; gap: 16px; color: var(--muted); font-size: 0.9rem; }
label.scheme-pick { margin: 0; }
label.scheme-pick select {
  width: auto; padding: 4px 8px; border: 1px solid var(--line);
  border-radius: 8px; background: var(--bg); color: var(--ink); font: inherit;
  font-size: 0.85rem;
}
.muted { color: var(--muted); }
.error { color: var(--accent); margin: 0 0 12px; }
.login-wrap { max-width: 420px; margin: 12px auto 0; }
.login-wrap h2 { margin: 0 0 8px; }
.login-wrap form p { margin: 0 0 12px; }
.login-wrap button { width: 100%; margin-top: 4px; }
.card {
  background: var(--paper);
  border: 1px solid var(--line);
  border-radius: 12px;
  padding: 8px 8px;
  margin: 0 0 16px;
}
.card.pad { padding: 18px 20px; }
label { display: block; font-size: 0.85rem; color: var(--muted); margin-bottom: 6px; }
input[type=password], input[type=text], input[type=file], input[type=search] {
  width: 100%; padding: 10px 12px; border: 1px solid var(--line);
  border-radius: 8px; background: var(--bg); color: var(--ink); font: inherit;
}
button, .btn {
  display: inline-block; border: 0; border-radius: 8px; padding: 9px 14px;
  background: var(--ink); color: var(--bg); font: inherit; cursor: pointer;
  text-decoration: none;
}
button.ghost, a.ghost {
  background: transparent; color: var(--ink); border: 1px solid var(--line);
}
button.danger { background: var(--accent); color: #fffdf8; }
.row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
.toolbar {
  display: flex; flex-wrap: wrap; gap: 10px; align-items: center;
  justify-content: space-between; margin-bottom: 14px;
}
.toolbar form { display: flex; gap: 8px; flex: 1; min-width: 220px; }
.toolbar form input { flex: 1; }
.kinds { display: flex; flex-wrap: wrap; gap: 6px; }
.kinds a {
  text-decoration: none; color: var(--muted); background: var(--chip);
  border-radius: 999px; padding: 4px 10px; font-size: 0.8rem;
}
.kinds a.on { background: var(--ink); color: var(--bg); }
.count { font-size: 0.85rem; color: var(--muted); margin: 0 0 10px; }
ul.docs { list-style: none; padding: 0; margin: 0; }
ul.docs li { border-bottom: 1px solid var(--line); }
ul.docs li:last-child { border-bottom: 0; }
ul.docs a.row-link {
  display: grid; grid-template-columns: 52px 1fr auto; gap: 12px;
  align-items: center; padding: 12px 12px; text-decoration: none; color: inherit;
  border-radius: 8px;
}
ul.docs a.row-link:hover { background: var(--bg); }
.badge {
  display: grid; place-items: center; width: 44px; height: 44px;
  border-radius: 10px; background: var(--chip); color: var(--muted);
  font-size: 0.68rem; font-weight: 700; letter-spacing: 0.04em;
}
ul.docs .title { font-weight: 650; }
ul.docs .sub { color: var(--muted); font-size: 0.88rem; }
.when { color: var(--muted); font-size: 0.82rem; white-space: nowrap; }
article.doc {
  background: var(--paper);
  border: 1px solid var(--line);
  border-radius: 12px;
  padding: 8px 36px 40px;
  font-family: Iowan Old Style, Palatino, Palatino Linotype, Book Antiqua, Georgia, serif;
  font-size: 1.05rem; line-height: 1.65;
  min-width: 0;
}
article.doc h1, article.doc h2, article.doc h3, article.doc h4 { line-height: 1.25; scroll-margin-top: 18px; }
article.doc h1 { font-size: 2rem; margin: 1.4em 0 0.5em; }
article.doc h2 { font-size: 1.45rem; margin: 1.5em 0 0.45em; padding-bottom: 0.2em; border-bottom: 1px solid var(--line); }
article.doc h3 { font-size: 1.18rem; margin: 1.3em 0 0.4em; }
article.doc h4 { font-size: 1.05rem; margin: 1.2em 0 0.35em; }
article.doc p { margin: 0.85em 0; overflow-wrap: anywhere; word-break: break-word; }
article.doc li, article.doc dd { overflow-wrap: anywhere; word-break: break-word; }
article.doc a { text-underline-offset: 2px; overflow-wrap: anywhere; word-break: break-word; }
article.doc hr { border: 0; border-top: 1px solid var(--line); margin: 2em 0; }
article.doc pre, article.doc code, pre.plain, .code-meta { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.88rem; }
article.doc :not(pre) > code {
  background: var(--chip); border-radius: 5px; padding: 0.12em 0.38em; font-size: 0.86em;
  overflow-wrap: anywhere; word-break: break-word; white-space: pre-wrap;
}
article.doc pre, pre.plain {
  background: var(--bg); border: 1px solid var(--line); border-radius: 8px;
  padding: 12px 14px; overflow: auto; -webkit-overflow-scrolling: touch;
  max-width: 100%;
}
article.doc .code-block {
  margin: 1.1em 0; border: 1px solid var(--line); border-radius: 10px; overflow: hidden;
}
article.doc .code-meta {
  display: flex; justify-content: space-between; align-items: center;
  padding: 6px 12px; background: var(--chip); color: var(--muted); font-size: 0.75rem;
  letter-spacing: 0.04em; text-transform: uppercase;
}
article.doc .code-block pre.syn-code {
  margin: 0; border: 0; border-radius: 0; padding: 14px 16px; overflow: auto;
  font-size: 0.86rem; line-height: 1.55;
}
article.doc .code-block pre.syn-code code { font: inherit; background: none; padding: 0; }
article.doc .copy-btn {
  float: right; margin: 8px 8px 0 0; border: 1px solid var(--line); background: var(--paper);
  color: var(--muted); border-radius: 6px; padding: 3px 8px; font-size: 0.72rem; cursor: pointer;
}
article.doc .mermaid-wrap {
  margin: 1.1em 0; padding: 16px; border: 1px solid var(--line); border-radius: 10px;
  background: var(--bg); overflow: auto; -webkit-overflow-scrolling: touch; text-align: center;
  max-width: 100%;
}
article.doc .mermaid-wrap svg { max-width: 100%; height: auto; }
article.doc pre.mermaid { background: transparent; border: 0; text-align: left; }
article.doc img, article.doc video, article.doc svg { max-width: 100%; height: auto; border-radius: 8px; }
.table-wrap {
  width: 100%; max-width: 100%; overflow-x: auto; -webkit-overflow-scrolling: touch;
  overscroll-behavior-x: contain; margin: 1em 0;
  border: 1px solid var(--line); border-radius: 8px; background: var(--paper);
}
article.doc table {
  border-collapse: collapse; width: max-content; min-width: 100%; margin: 0;
  font-size: 0.88rem; font-family: ui-sans-serif, system-ui, sans-serif;
}
article.doc th, article.doc td {
  border: 1px solid var(--line); padding: 8px 10px; vertical-align: top;
  overflow-wrap: anywhere; word-break: break-word; max-width: 16rem;
}
article.doc th { background: var(--chip); text-align: left; }
article.doc tr:nth-child(even) td { background: color-mix(in srgb, var(--chip) 55%, transparent); }
article.doc blockquote {
  margin: 1em 0; padding: 0.2em 0.8em 0.2em 1em; border-left: 3px solid var(--line);
  color: var(--muted); min-width: 0; overflow-wrap: anywhere; word-break: break-word;
}
article.doc blockquote.markdown-alert-note,
article.doc blockquote.markdown-alert-tip,
article.doc blockquote.markdown-alert-important,
article.doc blockquote.markdown-alert-warning,
article.doc blockquote.markdown-alert-caution {
  border-radius: 0 8px 8px 0; padding: 0.6em 1em; color: var(--ink);
}
article.doc blockquote.markdown-alert-note { border-left-color: #3b82f6; background: color-mix(in srgb, #3b82f6 10%, var(--paper)); }
article.doc blockquote.markdown-alert-tip { border-left-color: #16a34a; background: color-mix(in srgb, #16a34a 10%, var(--paper)); }
article.doc blockquote.markdown-alert-important { border-left-color: #8b5cf6; background: color-mix(in srgb, #8b5cf6 10%, var(--paper)); }
article.doc blockquote.markdown-alert-warning { border-left-color: #d97706; background: color-mix(in srgb, #d97706 12%, var(--paper)); }
article.doc blockquote.markdown-alert-caution { border-left-color: #dc2626; background: color-mix(in srgb, #dc2626 10%, var(--paper)); }
article.doc ul.contains-task-list { list-style: none; padding-left: 0.4em; }
article.doc input[type=checkbox] { margin-right: 0.45em; }
article.doc kbd {
  font-family: ui-monospace, Menlo, Consolas, monospace; font-size: 0.8em;
  border: 1px solid var(--line); border-bottom-width: 2px; border-radius: 5px;
  padding: 0.05em 0.4em; background: var(--chip);
}
article.doc dt { font-weight: 650; margin-top: 0.8em; }
article.doc dd { margin-left: 1.2em; color: var(--muted); }
article.doc .footnotes { margin-top: 2.2em; padding-top: 0.8em; border-top: 1px solid var(--line); font-size: 0.92em; color: var(--muted); }
.doc-layout { display: grid; grid-template-columns: minmax(0, 1fr); gap: 22px; align-items: start; }
.doc-layout > nav.toc {
  background: var(--paper); border: 1px solid var(--line); border-radius: 12px;
  padding: 10px 14px 14px; font-family: ui-sans-serif, system-ui, sans-serif; font-size: 0.86rem;
}
.doc-layout > nav.toc summary { cursor: pointer; color: var(--muted); font-weight: 650; letter-spacing: 0.02em; }
.doc-layout > nav.toc ol { list-style: none; padding: 8px 0 0; margin: 0; }
.doc-layout > nav.toc li { margin: 0; }
.doc-layout > nav.toc a {
  display: block; color: var(--muted); text-decoration: none; padding: 4px 0;
  border-left: 2px solid transparent; padding-left: 8px;
}
.doc-layout > nav.toc a:hover, .doc-layout > nav.toc a.active { color: var(--ink); border-left-color: var(--accent); }
.doc-layout > nav.toc li.l3 a { padding-left: 18px; }
.doc-layout > nav.toc li.l4 a, .doc-layout > nav.toc li.l5 a, .doc-layout > nav.toc li.l6 a { padding-left: 28px; }
@media (min-width: 960px) {
  .doc-layout.has-toc { grid-template-columns: 230px minmax(0, 1fr); }
  .doc-layout > nav.toc { position: sticky; top: 16px; max-height: calc(100vh - 32px); overflow: auto; }
  .doc-layout > nav.toc summary { pointer-events: none; }
}
.doc-layout.has-toc article.doc nav.toc,
.doc-layout.has-toc article.doc .toc { display: none; }
.doc-head { margin-bottom: 18px; }
.doc-head h2 { margin: 8px 0 6px; font-size: 1.6rem; }
.meta-line { font-size: 0.88rem; color: var(--muted); margin: 0 0 12px; }
.actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
.pager { display: flex; justify-content: space-between; gap: 12px; margin: 22px 0 0; font-size: 0.9rem; }
.pager .spacer { flex: 1; }
embed.pdf, iframe.pdf { width: 100%; height: 80vh; border: 1px solid var(--line); border-radius: 12px; background: var(--paper); }
img.preview { max-width: 100%; border-radius: 12px; border: 1px solid var(--line); }
footer.app { color: var(--muted); font-size: 0.8rem; padding: 36px 0 48px; }
.empty { padding: 36px 16px; color: var(--muted); text-align: center; }
code.inline { font-family: ui-monospace, Menlo, Consolas, monospace; font-size: 0.86em; }
.projects { display: flex; flex-wrap: wrap; gap: 6px; margin: 0 0 10px; }
.projects a {
  text-decoration: none; color: var(--ink); background: var(--chip);
  border-radius: 999px; padding: 4px 10px; font-size: 0.8rem;
}
.projects a.on { background: var(--accent); color: #fffdf8; }
.project-block { margin: 0 0 22px; }
.project-head {
  display: flex; align-items: baseline; justify-content: space-between;
  padding: 4px 8px 8px;
}
.project-head h2 { margin: 0; font-size: 1.05rem; font-weight: 650; }
.project-head h2 a { color: inherit; text-decoration: none; }
.project-head h2 a:hover { color: var(--accent); }
.project-head .n { color: var(--muted); font-size: 0.82rem; }
label.theme-pick { display: flex; align-items: center; gap: 8px; margin: 0; color: var(--muted); font-size: 0.85rem; }
label.theme-pick select {
  width: auto; padding: 6px 8px; border: 1px solid var(--line);
  border-radius: 8px; background: var(--bg); color: var(--ink); font: inherit;
}
@media (max-width: 720px) {
  header.app, footer.app, main,
  body.doc-page header.app, body.doc-page footer.app, body.doc-page main {
    width: calc(100% - 16px);
  }
  header.app { padding: 16px 0 12px; gap: 10px; flex-wrap: wrap; }
  header.app nav { gap: 12px; }
  header.app .header-end { gap: 12px; }
  article.doc { padding: 4px 14px 28px; font-size: 1rem; border-radius: 10px; }
  article.doc h1 { font-size: 1.45rem; }
  article.doc h2 { font-size: 1.2rem; }
  article.doc h3 { font-size: 1.08rem; }
  article.doc th, article.doc td { max-width: 11rem; padding: 6px 8px; font-size: 0.82rem; }
  .doc-head h2 { font-size: 1.28rem; overflow-wrap: anywhere; }
  .meta-line { overflow-wrap: anywhere; }
  .toolbar form { min-width: 0; width: 100%; }
  ul.docs a.row-link { grid-template-columns: 44px 1fr; }
  .when { grid-column: 2; white-space: normal; }
  .pager { flex-wrap: wrap; }
  label.theme-pick { width: 100%; }
  label.theme-pick select { flex: 1; min-width: 0; }
}
"#;

fn shell(title: &str, signed_in: bool, body: &str) -> String {
    shell_ex(title, signed_in, body, "", "", "")
}

fn shell_ex(
    title: &str,
    signed_in: bool,
    body: &str,
    body_class: &str,
    extra_css: &str,
    extra_head: &str,
) -> String {
    let nav = if signed_in {
        r#"<nav>
    <a href="/">inbox</a>
    <a href="/new">drop</a>
    <a href="/logout">logout</a>
  </nav>"#
    } else {
        ""
    };
    let body_attr = if body_class.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", esc(body_class))
    };
    format!(
        r#"<!doctype html>
<html lang="en" data-theme="auto" data-code-theme="auto">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>{title}</title>
<style>{CSS}{extra_css}</style>
<script src="/static/theme.js"></script>
{extra_head}
</head>
<body{body_attr}>
<header class="app">
  <h1><a href="/">gist</a></h1>
  <div class="header-end">
    <label class="scheme-pick">
      <select id="color-scheme" aria-label="Color scheme">
        <option value="auto">Auto</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
    {nav}
  </div>
</header>
<main>
{body}
</main>
<footer class="app">Private inbox. Agents write with <code class="inline">Authorization: Bearer</code>.</footer>
</body>
</html>"#,
        title = esc(title),
        CSS = CSS,
        extra_css = extra_css,
        extra_head = extra_head,
        nav = nav,
        body_attr = body_attr,
        body = body
    )
}

pub fn login(error: Option<&str>, next: Option<&str>) -> String {
    let err = error
        .map(|e| format!("<p class=\"error\" role=\"alert\">{e}</p>"))
        .unwrap_or_default();
    let next_field = next
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "/")
        .map(|s| format!(r#"<input type="hidden" name="next" value="{}">"#, esc(s)))
        .unwrap_or_default();
    shell(
        "gist — sign in",
        false,
        &format!(
            r#"
<div class="login-wrap">
<h2>Sign in</h2>
<p class="muted">This inbox is private. Use the same token agents use to write.</p>
<div class="card pad">
<form method="post" action="/login" autocomplete="on">
  {next_field}
  {err}
  <label for="username">Username</label>
  <input id="username" name="username" type="text" value="gist" autocomplete="username">
  <p></p>
  <label for="token">Token</label>
  <input id="token" name="token" type="password" autocomplete="current-password" autofocus required>
  <p class="muted">The browser can save this like any other site password.</p>
  <button type="submit">Sign in</button>
</form>
</div>
</div>
"#
        ),
    )
}

pub fn index(
    docs: &[Meta],
    q: &str,
    kind: Option<Kind>,
    project: Option<&str>,
    projects: &[String],
    total: usize,
) -> String {
    let proj_q = project.unwrap_or("");
    let filters = Kind::FILTERS
        .iter()
        .map(|k| {
            let on = if kind == Some(*k) { " on" } else { "" };
            format!(
                r#"<a class="{on}" href="/?q={q}&kind={key}&project={proj}">{label}</a>"#,
                on = on.trim(),
                q = qenc(q),
                key = k.filter_key(),
                proj = qenc(proj_q),
                label = esc(k.label()),
            )
        })
        .collect::<String>();
    let all_on = if kind.is_none() { " on" } else { "" };

    let project_chips = if projects.is_empty() {
        String::new()
    } else {
        let chips: String = projects
            .iter()
            .map(|p| {
                let on = if project == Some(p.as_str()) {
                    " on"
                } else {
                    ""
                };
                format!(
                    r#"<a class="{on}" href="/?q={q}&project={p}{kindq}">{label}</a>"#,
                    on = on.trim(),
                    q = qenc(q),
                    p = qenc(p),
                    kindq = kind
                        .map(|k| format!("&kind={}", k.filter_key()))
                        .unwrap_or_default(),
                    label = esc(p),
                )
            })
            .collect();
        let all_p = if project.is_none() { " on" } else { "" };
        format!(
            r#"<nav class="projects">
  <a class="{all_p}" href="/?q={q}{kindq}">All projects</a>
  {chips}
</nav>"#,
            all_p = all_p.trim(),
            q = qenc(q),
            kindq = kind
                .map(|k| format!("&kind={}", k.filter_key()))
                .unwrap_or_default(),
            chips = chips,
        )
    };

    let list = if docs.is_empty() {
        if total == 0 {
            "<p class=\"empty\">Nothing here yet. Agents can drop files with <code class=\"inline\">PUT /d/&lt;project&gt;/&lt;slug&gt;</code>, or use <a href=\"/new\">drop</a>.</p>".to_string()
        } else {
            "<p class=\"empty\">No documents match this search.</p>".to_string()
        }
    } else {
        grouped_lists(docs)
    };

    let count = if docs.len() == total {
        format!("{} document{}", total, if total == 1 { "" } else { "s" })
    } else {
        format!("{} of {} documents", docs.len(), total)
    };

    shell(
        "gist",
        true,
        &format!(
            r#"
<div class="toolbar">
  <form method="get" action="/">
    <input type="search" name="q" value="{q}" placeholder="Search title, slug, or project" aria-label="Search">
    {kind_hidden}
    {project_hidden}
    <button type="submit">Search</button>
  </form>
</div>
{project_chips}
<nav class="kinds">
  <a class="{all_on}" href="/?q={qenc}&project={proj}">{all_label}</a>
  {filters}
</nav>
<p class="count">{count}</p>
{list}
"#,
            q = esc(q),
            qenc = qenc(q),
            proj = qenc(proj_q),
            all_label = if project.is_some() {
                "All types"
            } else {
                "All"
            },
            kind_hidden = kind
                .map(|k| format!(
                    r#"<input type="hidden" name="kind" value="{}">"#,
                    k.filter_key()
                ))
                .unwrap_or_default(),
            project_hidden = project
                .map(|p| format!(r#"<input type="hidden" name="project" value="{}">"#, esc(p)))
                .unwrap_or_default(),
            all_on = all_on.trim(),
            filters = filters,
            project_chips = project_chips,
            count = esc(&count),
            list = list,
        ),
    )
}

fn grouped_lists(docs: &[Meta]) -> String {
    let mut order: Vec<String> = Vec::new();
    for d in docs {
        if !order.iter().any(|p| p == &d.project) {
            order.push(d.project.clone());
        }
    }
    order
        .iter()
        .map(|project| {
            let items: Vec<&Meta> = docs.iter().filter(|d| &d.project == project).collect();
            let rows: String = items.iter().map(|d| doc_row(d)).collect();
            format!(
                r#"<section class="project-block">
  <div class="project-head">
    <h2><a href="/?project={p}">{label}</a></h2>
    <span class="n">{n}</span>
  </div>
  <div class="card"><ul class="docs">{rows}</ul></div>
</section>"#,
                p = qenc(project),
                label = esc(project),
                n = items.len(),
                rows = rows,
            )
        })
        .collect()
}

fn doc_row(d: &Meta) -> String {
    let k = view_kind(&d.content_type, &d.slug);
    format!(
        r#"<li>
  <a class="row-link" href="{href}">
    <span class="badge">{badge}</span>
    <span>
      <span class="title">{title}</span>
      <div class="sub">{slug} · {size}</div>
    </span>
    <span class="when">{when}</span>
  </a>
</li>"#,
        href = esc(&d.href()),
        badge = esc(k.short()),
        title = esc(&d.title),
        slug = esc(&d.slug),
        size = esc(&human_size(d.bytes)),
        when = esc(&human_time(&d.updated)),
    )
}

pub fn drop_form() -> String {
    shell(
        "gist — drop",
        true,
        r#"
<h2>Drop a document</h2>
<p class="muted">For agents, prefer a raw PUT. This form is for you.</p>
<div class="card pad">
<form method="post" action="/d" enctype="multipart/form-data">
  <label for="project">Project</label>
  <input id="project" name="project" type="text" value="inbox" placeholder="inbox">
  <p></p>
  <label for="slug">Slug (optional, e.g. notes.md)</label>
  <input id="slug" name="slug" type="text" placeholder="auto-generated if empty">
  <p></p>
  <label for="title">Title (optional)</label>
  <input id="title" name="title" type="text">
  <p></p>
  <label for="file">File</label>
  <input id="file" name="file" type="file" required>
  <p></p>
  <button type="submit">Save</button>
</form>
</div>
"#,
    )
}

pub fn document(
    meta: &Meta,
    kind: Kind,
    rendered: &Rendered,
    newer: Option<&Meta>,
    older: Option<&Meta>,
) -> String {
    let extra = match kind {
        Kind::Image => format!(
            r#"<p><img class="preview" src="{href}/raw" alt="{title}"></p>"#,
            href = esc(&meta.href()),
            title = esc(&meta.title)
        ),
        Kind::Pdf => format!(
            r#"<embed class="pdf" src="{href}/raw" type="application/pdf">"#,
            href = esc(&meta.href())
        ),
        Kind::File => format!(
            r#"<div class="card pad"><p>Binary file ({ct}, {size}).</p>
               <p><a class="btn" href="{href}/raw">Download</a></p></div>"#,
            ct = esc(&meta.content_type),
            size = esc(&human_size(meta.bytes)),
            href = esc(&meta.href())
        ),
        _ => rendered.html.clone(),
    };

    let toc_html = toc_nav(&rendered.toc);
    let has_toc = !toc_html.is_empty();
    let article = if matches!(kind, Kind::Markdown | Kind::Html | Kind::Text) {
        format!("<article class=\"doc\">{}</article>", rendered.html)
    } else {
        extra
    };
    let body_html = if has_toc {
        format!(
            r#"<div class="doc-layout has-toc">{toc}{article}</div>"#,
            toc = toc_html,
            article = article
        )
    } else if matches!(kind, Kind::Markdown | Kind::Html | Kind::Text) {
        format!(r#"<div class="doc-layout">{article}</div>"#)
    } else {
        article
    };

    let theme_pick = if matches!(kind, Kind::Markdown) && rendered.has_code {
        let opts: String = CODE_THEMES
            .iter()
            .map(|t| {
                format!(
                    r#"<option value="{id}">{label}</option>"#,
                    id = esc(t.id),
                    label = esc(t.label)
                )
            })
            .collect();
        format!(
            r#"<label class="theme-pick">Code theme
  <select id="code-theme" aria-label="Code theme">{opts}</select>
</label>"#
        )
    } else {
        String::new()
    };

    let newer_link = newer
        .map(|d| {
            format!(
                r#"<a href="{href}">← {title}</a>"#,
                href = esc(&d.href()),
                title = esc(&d.title)
            )
        })
        .unwrap_or_else(|| r#"<span class="muted">Newest</span>"#.into());
    let older_link = older
        .map(|d| {
            format!(
                r#"<a href="{href}">{title} →</a>"#,
                href = esc(&d.href()),
                title = esc(&d.title)
            )
        })
        .unwrap_or_else(|| r#"<span class="muted">Oldest</span>"#.into());

    let extra_css = if matches!(kind, Kind::Markdown) && rendered.has_code {
        highlight::theme_css()
    } else {
        ""
    };
    let extra_head = if has_toc
        || (matches!(kind, Kind::Markdown) && (rendered.has_code || rendered.has_mermaid))
    {
        r#"<script src="/static/doc.js"></script>"#
    } else {
        ""
    };
    let mut body_class = "doc-page".to_string();
    if rendered.has_code {
        body_class.push_str(" has-code");
    }
    if rendered.has_mermaid {
        body_class.push_str(" has-mermaid");
    }

    shell_ex(
        &meta.title,
        true,
        &format!(
            r#"
<div class="doc-head">
  <a href="/?project={project}">← {project_label}</a>
  <h2>{title}</h2>
  <p class="meta-line">{kind} · {project_label}/{slug} · {size} · {when}</p>
  <div class="actions">
    <a class="btn ghost" href="{href}/raw">Raw</a>
    {theme_pick}
    <form method="post" action="{href}/delete">
      <button class="danger" type="submit">Delete</button>
    </form>
  </div>
</div>
{body_html}
<nav class="pager">
  {newer}
  <span class="spacer"></span>
  {older}
</nav>
"#,
            title = esc(&meta.title),
            kind = esc(kind.label()),
            project = qenc(&meta.project),
            project_label = esc(&meta.project),
            slug = esc(&meta.slug),
            href = esc(&meta.href()),
            size = esc(&human_size(meta.bytes)),
            when = esc(&human_time(&meta.updated)),
            theme_pick = theme_pick,
            body_html = body_html,
            newer = newer_link,
            older = older_link,
        ),
        &body_class,
        extra_css,
        extra_head,
    )
}

fn toc_nav(toc: &[TocItem]) -> String {
    // Skip a lone document title (h1) when there are real sections under it.
    let skip_h1 = toc.iter().any(|t| t.level == 1) && toc.iter().any(|t| t.level > 1);
    let items: Vec<&TocItem> = toc
        .iter()
        .filter(|t| t.level <= 4 && !(skip_h1 && t.level == 1))
        .collect();
    if items.len() < 2 {
        return String::new();
    }
    let lis: String = items
        .iter()
        .map(|t| {
            format!(
                r##"<li class="l{level}"><a href="#{id}">{text}</a></li>"##,
                level = t.level,
                id = esc(&t.id),
                text = esc(&t.text),
            )
        })
        .collect();
    format!(
        r#"<nav class="toc" aria-label="On this page">
  <details open>
    <summary>On this page</summary>
    <ol>{lis}</ol>
  </details>
</nav>"#
    )
}

pub fn not_found() -> String {
    shell(
        "gist — missing",
        true,
        "<h2>Not found</h2><p class=\"muted\">No document with that slug. <a href=\"/\">Back to inbox</a>.</p>",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_include_appearance_switch() {
        let html = login(None, None);
        assert!(html.contains("/static/theme.js"));
        assert!(html.contains("id=\"color-scheme\""));
        assert!(html.contains("html[data-theme=\"dark\"]"));
        assert!(html.contains("html[data-theme=\"auto\"]"));
    }

    #[test]
    fn toc_sticky_styles_are_scoped_to_sidebar() {
        let html = login(None, None);
        assert!(html.contains(".doc-layout > nav.toc { position: sticky;"));
        assert!(!html.contains("\n.toc { position: sticky;"));
        assert!(html.contains(".doc-layout.has-toc article.doc .toc { display: none; }"));
    }

    #[test]
    fn html_document_uses_sidebar_toc() {
        let meta = Meta {
            project: "inbox".into(),
            slug: "cutover.html".into(),
            title: "Cutover".into(),
            content_type: "text/html".into(),
            bytes: 200,
            created: OffsetDateTime::now_utc(),
            updated: OffsetDateTime::now_utc(),
        };
        let rendered = crate::render::Rendered {
            html: r##"<nav class="toc"><a href="#s1">Problem</a></nav><h1 id="cutover">Cutover</h1><h2 id="s1">Problem</h2><h2 id="s2">Plan</h2>"##.into(),
            toc: vec![
                TocItem {
                    level: 1,
                    id: "cutover".into(),
                    text: "Cutover".into(),
                },
                TocItem {
                    level: 2,
                    id: "s1".into(),
                    text: "Problem".into(),
                },
                TocItem {
                    level: 2,
                    id: "s2".into(),
                    text: "Plan".into(),
                },
            ],
            has_mermaid: false,
            has_code: false,
        };
        let html = document(&meta, Kind::Html, &rendered, None, None);
        assert!(html.contains("class=\"doc-layout has-toc\""));
        assert!(html.contains("On this page"));
        assert!(html.contains("/static/doc.js"));
        assert!(html.contains("<article class=\"doc\">"));
        assert!(html.contains("nav class=\"toc\""));
    }
}

fn human_size(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

fn human_time(t: &OffsetDateTime) -> String {
    let now = OffsetDateTime::now_utc();
    let hm = format_description!("[hour]:[minute]");
    let day = format_description!("[day padding:none] [month repr:short] [year]");
    let clock = t.format(hm).unwrap_or_default();
    if t.date() == now.date() {
        format!("today {clock}")
    } else if now.date().previous_day() == Some(t.date()) {
        format!("yesterday {clock}")
    } else {
        t.format(day)
            .unwrap_or_else(|_| t.format(&Rfc3339).unwrap_or_else(|_| t.to_string()))
    }
}

fn qenc(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            c => out.push_str(&format!("%{c:02X}")),
        }
    }
    out
}
