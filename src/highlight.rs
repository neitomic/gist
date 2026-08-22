use std::sync::OnceLock;

use syntect::highlighting::ThemeSet;
use syntect::html::{css_for_theme_with_class_style, ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::render::esc;

const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "syn-" };

pub struct CodeTheme {
    pub id: &'static str,
    pub label: &'static str,
    pub syntect: Option<&'static str>,
}

/// Selectable code themes. `auto` follows the page color scheme.
pub const CODE_THEMES: &[CodeTheme] = &[
    CodeTheme {
        id: "auto",
        label: "Auto",
        syntect: None,
    },
    CodeTheme {
        id: "github",
        label: "GitHub Light",
        syntect: Some("InspiredGitHub"),
    },
    CodeTheme {
        id: "ocean-dark",
        label: "Ocean Dark",
        syntect: Some("base16-ocean.dark"),
    },
    CodeTheme {
        id: "ocean-light",
        label: "Ocean Light",
        syntect: Some("base16-ocean.light"),
    },
    CodeTheme {
        id: "mocha",
        label: "Mocha Dark",
        syntect: Some("base16-mocha.dark"),
    },
    CodeTheme {
        id: "eighties",
        label: "Eighties Dark",
        syntect: Some("base16-eighties.dark"),
    },
    CodeTheme {
        id: "solarized-dark",
        label: "Solarized Dark",
        syntect: Some("Solarized (dark)"),
    },
    CodeTheme {
        id: "solarized-light",
        label: "Solarized Light",
        syntect: Some("Solarized (light)"),
    },
];

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn themes() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

pub fn theme_css() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        let set = themes();
        let mut css = String::new();
        for theme in CODE_THEMES {
            let Some(name) = theme.syntect else {
                continue;
            };
            let Some(tm) = set.themes.get(name) else {
                continue;
            };
            let Ok(raw) = css_for_theme_with_class_style(tm, CLASS_STYLE) else {
                continue;
            };
            css.push_str(&scope_css(
                &format!("html[data-code-theme=\"{}\"]", theme.id),
                &raw,
            ));
        }
        // Auto: GitHub in light, Ocean Dark in dark.
        if let Some(light) = set.themes.get("InspiredGitHub") {
            if let Ok(raw) = css_for_theme_with_class_style(light, CLASS_STYLE) {
                css.push_str(&scope_css("html[data-code-theme=\"auto\"]", &raw));
            }
        }
        if let Some(dark) = set.themes.get("base16-ocean.dark") {
            if let Ok(raw) = css_for_theme_with_class_style(dark, CLASS_STYLE) {
                css.push_str("@media (prefers-color-scheme: dark) {\n");
                css.push_str(&scope_css("html[data-code-theme=\"auto\"]", &raw));
                css.push_str("}\n");
            }
        }
        css
    })
}

fn scope_css(prefix: &str, css: &str) -> String {
    let mut out = String::with_capacity(css.len() + 64);
    for line in css.lines() {
        let trimmed = line.trim_start();
        if let Some(brace) = trimmed.find('{') {
            let selectors = &trimmed[..brace];
            if !selectors.is_empty() && !selectors.starts_with('@') {
                let scoped: Vec<String> = selectors
                    .split(',')
                    .map(|s| format!("{prefix} {}", s.trim()))
                    .collect();
                out.push_str(&scoped.join(", "));
                out.push(' ');
                out.push_str(&trimmed[brace..]);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub fn code_html(lang: &str, code: &str) -> String {
    let inner = highlight(lang, code);
    let lang_label = format!(
        "<div class=\"code-meta\"><span>{}</span></div>",
        esc(if lang.is_empty() { "code" } else { lang })
    );
    format!(
        "<div class=\"code-block\">{meta}<pre class=\"syn-code\"><code>{inner}</code></pre></div>\n",
        meta = lang_label,
        inner = inner
    )
}

fn highlight(lang: &str, code: &str) -> String {
    let ss = syntaxes();
    let syntax = if lang.is_empty() {
        ss.find_syntax_plain_text()
    } else {
        ss.find_syntax_by_token(lang)
            .or_else(|| ss.find_syntax_by_extension(lang))
            .unwrap_or_else(|| ss.find_syntax_plain_text())
    };
    let mut gen = ClassedHTMLGenerator::new_with_class_style(syntax, ss, CLASS_STYLE);
    let src = if code.ends_with('\n') {
        code.to_string()
    } else {
        format!("{code}\n")
    };
    for line in LinesWithEndings::from(&src) {
        let _ = gen.parse_html_for_line_which_includes_newline(line);
    }
    gen.finalize()
}
