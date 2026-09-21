mod agents;
mod auth;
mod cli;
mod config;
mod highlight;
mod pages;
mod pdf;
mod render;
mod service;
mod store;

use auth::{extract_from_headers, Authed, Token};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use bytes::Bytes;
use clap::Parser;
use render::{render_body, view_kind, Kind, Rendered, View};
use serde::Deserialize;
use std::{
    io::{ErrorKind, Write},
    net::SocketAddr,
    path::PathBuf,
};
use store::{guess_content_type, guess_title, Store, StoreError, DEFAULT_PROJECT};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use tokio::net::TcpListener;
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing::{info, warn};

const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;
const COOKIE_MAX_AGE: Duration = Duration::days(400);

#[derive(Clone)]
struct App {
    store: Store,
    token: Token,
    max_bytes: usize,
    public_base: Option<String>,
}

impl axum::extract::FromRef<App> for Token {
    fn from_ref(app: &App) -> Token {
        app.token.clone()
    }
}

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Serve { yes } => serve(yes).await,
        cmd => {
            if let Err(e) = cli::run(cmd) {
                eprintln!("gist: {e}");
                std::process::exit(1);
            }
        }
    }
}

async fn serve(assume_yes: bool) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gist=info,tower_http=info".into()),
        )
        .init();

    let bind: SocketAddr = std::env::var("GIST_BIND")
        .unwrap_or_else(|_| config::DEFAULT_BIND.into())
        .parse()
        .expect("GIST_BIND must be host:port");

    let data = PathBuf::from(std::env::var("GIST_DATA").unwrap_or_else(|_| "data".into()));
    std::fs::create_dir_all(&data).expect("create data dir");

    let max_bytes = std::env::var("GIST_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_BYTES);

    let token = load_or_create_token(&data);
    let public_base = std::env::var("GIST_PUBLIC_URL").ok();
    let existing = config::load_file(&config::path()).unwrap_or_default();
    if let Some(target) = config::client_clash(&existing, &token) {
        if !confirm_client_overwrite(&target, assume_yes) {
            std::process::exit(1);
        }
    }
    let agent_cfg =
        config::apply_listen(existing, &bind.to_string(), public_base.as_deref(), &token);
    match config::save(&agent_cfg) {
        Ok(path) => info!("agent credentials written to {}", path.display()),
        Err(e) => warn!("could not write {}: {e}", config::path().display()),
    }
    let agent_url = agent_cfg.env_url().to_string();
    let cli_url = agent_cfg.cli_url().to_string();

    let app = App {
        store: Store::new(data.clone()),
        token: Token(token.clone()),
        max_bytes,
        public_base,
    };

    let router = router(app.clone());

    info!("listening on http://{bind}");
    info!("data dir {}", data.display());
    info!("agents: gist put FILE   or   GET {agent_url}/agent");
    eprintln!();
    eprintln!("  browse:  {cli_url}");
    eprintln!("  drop:    gist put notes.md");
    eprintln!("  env:     gist env");
    if cli_url != agent_url {
        eprintln!("  remote:  {agent_url}");
    }
    eprintln!();

    let listener = TcpListener::bind(bind).await.expect("bind");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown())
        .await
        .expect("server");
}

fn router(app: App) -> Router {
    let limit = app.max_bytes;
    Router::new()
        .route("/health", get(health))
        .route("/agent", get(agent_card))
        .route("/agent.md", get(agent_markdown))
        .route("/", get(home))
        .route("/login", get(login_get).post(login_post))
        .route("/logout", get(logout))
        .route("/new", get(new_get))
        .route("/static/doc.js", get(doc_js))
        .route("/static/theme.js", get(theme_js))
        .route("/d", post(create_multipart))
        .route("/d/{slug}", get(view_doc).put(put_doc))
        .route("/d/{slug}/raw", get(raw_doc))
        .route("/d/{slug}/pdf", get(pdf_doc))
        .route("/d/{slug}/delete", post(delete_doc))
        .route("/d/{project}/{slug}", get(view_doc_p).put(put_doc_p))
        .route("/d/{project}/{slug}/raw", get(raw_doc_p))
        .route("/d/{project}/{slug}/pdf", get(pdf_doc_p))
        .route("/d/{project}/{slug}/delete", post(delete_doc_p))
        .route("/api/docs", get(api_list).post(api_create))
        .route(
            "/api/docs/{slug}",
            get(api_get).put(put_doc).delete(delete_doc_api),
        )
        .route(
            "/api/docs/{project}/{slug}",
            get(api_get_p).put(put_doc_p).delete(delete_doc_api_p),
        )
        .layer(middleware::from_fn(security_headers))
        .layer(DefaultBodyLimit::max(limit))
        .layer(RequestBodyLimitLayer::new(limit))
        .layer(TraceLayer::new_for_http())
        .with_state(app)
}

async fn security_headers(req: Request<Body>, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("SAMEORIGIN"),
    );
    h.insert(
        header::HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        header::HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' data: https:; style-src 'unsafe-inline'; script-src 'self' https://cdn.jsdelivr.net 'wasm-unsafe-eval'; object-src 'self'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    h.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    res
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutting down");
}

/// `serve` rewrites the config file that `gist put` reads. When that file is
/// already a client for another server, say so before replacing its token.
fn confirm_client_overwrite(target: &str, assume_yes: bool) -> bool {
    use std::io::{BufRead, IsTerminal};

    let path = config::path();
    eprintln!("gist: {} is a client for {target}", path.display());
    eprintln!("gist: starting a server here replaces that token; `gist put` and `gist list` will stop working against {target}");

    if assume_yes || std::env::var("GIST_ASSUME_YES").is_ok() {
        eprintln!("gist: continuing (--yes)");
        return true;
    }
    if !std::io::stdin().is_terminal() {
        eprintln!("gist: refusing to overwrite it without a terminal to ask. Pass --yes, set GIST_ASSUME_YES=1, or point GIST_CONFIG somewhere else.");
        return false;
    }
    eprint!("Replace the token in {}? [y/N] ", path.display());
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn load_or_create_token(data: &std::path::Path) -> String {
    if let Ok(t) = std::env::var("GIST_TOKEN") {
        let t = t.trim().to_string();
        if t.len() < 16 {
            panic!("GIST_TOKEN must be at least 16 characters");
        }
        return t;
    }
    if let Some(cfg) = config::load_file(&config::path()) {
        if cfg.token.len() >= 16 {
            info!("using token from {}", config::path().display());
            return cfg.token;
        }
    }
    let path = data.join(".token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let t = existing.trim().to_string();
        if !t.is_empty() {
            info!("using token from {}", path.display());
            return t;
        }
    }
    let generated = format!("gst_{}", nanoid::nanoid!(32));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path);
    match f.as_mut() {
        Ok(file) => {
            writeln!(file, "{generated}").expect("write token");
            eprintln!("generated GIST_TOKEN (also written to {}):", path.display());
            eprintln!("  {generated}");
            eprintln!("set GIST_TOKEN in the environment to keep a stable token.");
            generated
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => std::fs::read_to_string(&path)
            .expect("read token")
            .trim()
            .to_string(),
        Err(e) => panic!("could not write {}: {e}", path.display()),
    }
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok\n")
}

fn js_file(body: &'static str) -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
        ],
        body,
    )
}

async fn doc_js() -> impl IntoResponse {
    js_file(include_str!("../static/doc.js"))
}

async fn theme_js() -> impl IntoResponse {
    js_file(include_str!("../static/theme.js"))
}

async fn agent_card(State(app): State<App>, headers: HeaderMap) -> impl IntoResponse {
    let base = origin(&app, &headers);
    Json(serde_json::json!({
        "name": "gist",
        "base": base,
        "auth": {
            "type": "bearer",
            "header": "Authorization",
            "scheme": "Bearer",
            "alt_header": "X-Gist-Token",
            "env": ["GIST_URL", "GIST_TOKEN"],
            "config": "~/.config/gist/config",
            "cli": "gist put FILE"
        },
        "put": {
            "method": "PUT",
            "path": "/d/{project}/{slug}",
            "headers": ["Content-Type", "X-Title", "X-Project"],
            "default_project": DEFAULT_PROJECT,
            "overwrite": true
        },
        "list": { "method": "GET", "path": "/api/docs" },
        "docs": "/agent.md"
    }))
}

async fn agent_markdown(State(app): State<App>, headers: HeaderMap) -> impl IntoResponse {
    let base = origin(&app, &headers);
    let body = format!(
        "Base URL for this server: {base}\n\n{}",
        include_str!("../AGENT.md")
    );
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/markdown; charset=utf-8"),
        )],
        body,
    )
}

#[derive(Debug, Default, Deserialize)]
struct BrowseQuery {
    q: Option<String>,
    kind: Option<String>,
    project: Option<String>,
}

async fn home(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<BrowseQuery>,
) -> Response {
    if !authed(&app, &headers) {
        return redirect_to_login("/");
    }
    match app.store.list().await {
        Ok(all) => {
            let q = query.q.as_deref().unwrap_or("").trim();
            let kind = query.kind.as_deref().and_then(Kind::from_filter);
            let project = query
                .project
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let q_lower = q.to_ascii_lowercase();
            let mut projects: Vec<String> = Vec::new();
            for d in &all {
                if !projects.iter().any(|p| p == &d.project) {
                    projects.push(d.project.clone());
                }
            }
            projects.sort();
            let shown: Vec<_> = all
                .iter()
                .filter(|d| {
                    if project.is_some_and(|want| d.project != want) {
                        return false;
                    }
                    let k = view_kind(&d.content_type, &d.slug);
                    if kind.is_some_and(|want| k != want) {
                        return false;
                    }
                    if q_lower.is_empty() {
                        return true;
                    }
                    d.title.to_ascii_lowercase().contains(&q_lower)
                        || d.slug.to_ascii_lowercase().contains(&q_lower)
                        || d.project.to_ascii_lowercase().contains(&q_lower)
                })
                .cloned()
                .collect();
            Html(pages::index(&shown, q, kind, project, &projects, all.len())).into_response()
        }
        Err(e) => {
            warn!("list failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "store error").into_response()
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct LoginQuery {
    next: Option<String>,
}

async fn login_get(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<LoginQuery>,
) -> Response {
    let next = safe_next(query.next.as_deref());
    if authed(&app, &headers) {
        return Redirect::to(&next).into_response();
    }
    Html(pages::login(None, Some(next.as_str()))).into_response()
}

#[derive(Deserialize)]
struct LoginForm {
    token: String,
    next: Option<String>,
}

async fn login_post(
    State(app): State<App>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    let next = safe_next(form.next.as_deref());
    if !app.token.matches(form.token.trim()) {
        return Html(pages::login(
            Some("That token is not accepted."),
            Some(next.as_str()),
        ))
        .into_response();
    }
    set_cookie_and_redirect(&app, &headers, form.token.trim(), &next)
}

async fn logout() -> Response {
    let mut cookie = Cookie::new(Token::cookie_name(), "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(Duration::ZERO);
    let mut res = Redirect::to("/login").into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie.to_string()).expect("cookie"),
    );
    res
}

async fn new_get(State(app): State<App>, headers: HeaderMap) -> Response {
    if !authed(&app, &headers) {
        return redirect_to_login("/new");
    }
    Html(pages::drop_form()).into_response()
}

async fn put_doc(
    State(app): State<App>,
    Authed: Authed,
    Path(slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    save_doc(app, project_from_headers(&headers), slug, headers, body).await
}

async fn put_doc_p(
    State(app): State<App>,
    Authed: Authed,
    Path((project, slug)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    save_doc(app, project, slug, headers, body).await
}

async fn save_doc(
    app: App,
    project: String,
    slug: String,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if body.len() > app.max_bytes {
        return (StatusCode::PAYLOAD_TOO_LARGE, "too large").into_response();
    }
    if Store::validate_project(&project).is_err() || Store::validate_slug(&slug).is_err() {
        return (StatusCode::BAD_REQUEST, "invalid project or slug").into_response();
    }
    let header_ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());
    let content_type = guess_content_type(&slug, header_ct);
    let title = headers
        .get("x-title")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| guess_title(&slug, &content_type, &body));

    match app
        .store
        .put(&project, &slug, title, content_type, &body)
        .await
    {
        Ok(meta) => {
            let url = public_doc_url(&app, &headers, &meta);
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "project": meta.project,
                    "slug": meta.slug,
                    "title": meta.title,
                    "content_type": meta.content_type,
                    "bytes": meta.bytes,
                    "url": url,
                    "raw_url": format!("{url}/raw"),
                    "updated": meta.updated.format(&Rfc3339).unwrap_or_default(),
                })),
            )
                .into_response()
        }
        Err(e) => store_error(e),
    }
}

async fn api_create(
    State(app): State<App>,
    Authed: Authed,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let slug = headers
        .get("x-slug")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(new_slug);
    let project = project_from_headers(&headers);
    save_doc(app, project, slug, headers, body).await
}

async fn create_multipart(
    State(app): State<App>,
    Authed: Authed,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let mut slug: Option<String> = None;
    let mut project: Option<String> = None;
    let mut title: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut body: Option<Bytes> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "project" => {
                if let Ok(v) = field.text().await {
                    if !v.trim().is_empty() {
                        project = Some(v.trim().to_string());
                    }
                }
            }
            "slug" => {
                if let Ok(v) = field.text().await {
                    if !v.trim().is_empty() {
                        slug = Some(v.trim().to_string());
                    }
                }
            }
            "title" => {
                if let Ok(v) = field.text().await {
                    if !v.trim().is_empty() {
                        title = Some(v.trim().to_string());
                    }
                }
            }
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                content_type = field.content_type().map(|s| s.to_string());
                match field.bytes().await {
                    Ok(b) => body = Some(b),
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, format!("upload failed: {e}"))
                            .into_response();
                    }
                }
            }
            _ => {}
        }
    }

    let Some(body) = body else {
        return (StatusCode::BAD_REQUEST, "missing file").into_response();
    };
    if body.len() > app.max_bytes {
        return (StatusCode::PAYLOAD_TOO_LARGE, "too large").into_response();
    }

    let slug = slug
        .or_else(|| filename.as_deref().and_then(safe_filename))
        .unwrap_or_else(new_slug);

    let project = project
        .or_else(|| {
            let h = project_from_headers(&headers);
            if h == DEFAULT_PROJECT {
                None
            } else {
                Some(h)
            }
        })
        .unwrap_or_else(|| DEFAULT_PROJECT.to_string());

    if Store::validate_project(&project).is_err() || Store::validate_slug(&slug).is_err() {
        return (StatusCode::BAD_REQUEST, "invalid project or slug").into_response();
    }

    let type_name = filename.as_deref().unwrap_or(slug.as_str());
    let ct = guess_content_type(type_name, content_type.as_deref());
    let title = title.unwrap_or_else(|| guess_title(&slug, &ct, &body));
    match app.store.put(&project, &slug, title, ct, &body).await {
        Ok(meta) => {
            if wants_html(&headers) {
                Redirect::to(&meta.href()).into_response()
            } else {
                let url = public_doc_url(&app, &headers, &meta);
                (
                    StatusCode::CREATED,
                    Json(serde_json::json!({
                        "project": meta.project,
                        "slug": meta.slug,
                        "title": meta.title,
                        "url": url
                    })),
                )
                    .into_response()
            }
        }
        Err(e) => store_error(e),
    }
}

async fn view_doc(
    State(app): State<App>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    show_doc(app, DEFAULT_PROJECT.to_string(), slug, headers).await
}

async fn view_doc_p(
    State(app): State<App>,
    Path((project, slug)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    show_doc(app, project, slug, headers).await
}

async fn show_doc(app: App, project: String, slug: String, headers: HeaderMap) -> Response {
    if !authed(&app, &headers) {
        let path = if project == DEFAULT_PROJECT {
            format!("/d/{slug}")
        } else {
            format!("/d/{project}/{slug}")
        };
        return redirect_to_login(&path);
    }
    let meta = match app.store.get_meta(&project, &slug).await {
        Ok(m) => m,
        Err(StoreError::NotFound | StoreError::BadSlug) => {
            return (StatusCode::NOT_FOUND, Html(pages::not_found())).into_response();
        }
        Err(e) => return store_error(e),
    };
    let body = match app.store.get_content(&project, &slug).await {
        Ok(b) => b,
        Err(e) => return store_error(e),
    };
    let kind = view_kind(&meta.content_type, &meta.slug);
    let rendered = match render_body(kind, &body) {
        View::Html(html) => html,
        View::Binary => Rendered::default(),
    };
    let docs = app.store.list().await.unwrap_or_default();
    let same: Vec<_> = docs
        .into_iter()
        .filter(|d| d.project == meta.project)
        .collect();
    let idx = same
        .iter()
        .position(|d| d.slug == meta.slug && d.project == meta.project);
    let newer = idx.and_then(|i| i.checked_sub(1).and_then(|j| same.get(j)));
    let older = idx.and_then(|i| same.get(i + 1));
    Html(pages::document(&meta, kind, &rendered, newer, older)).into_response()
}

async fn raw_doc(State(app): State<App>, Authed: Authed, Path(slug): Path<String>) -> Response {
    raw_body(app, DEFAULT_PROJECT.to_string(), slug).await
}

async fn pdf_doc(State(app): State<App>, Path(slug): Path<String>, headers: HeaderMap) -> Response {
    export_pdf(app, DEFAULT_PROJECT.to_string(), slug, headers).await
}

async fn pdf_doc_p(
    State(app): State<App>,
    Path((project, slug)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    export_pdf(app, project, slug, headers).await
}

async fn export_pdf(app: App, project: String, slug: String, headers: HeaderMap) -> Response {
    if !authed(&app, &headers) {
        let path = if project == DEFAULT_PROJECT {
            format!("/d/{slug}/pdf")
        } else {
            format!("/d/{project}/{slug}/pdf")
        };
        return redirect_to_login(&path);
    }
    let meta = match app.store.get_meta(&project, &slug).await {
        Ok(m) => m,
        Err(StoreError::NotFound | StoreError::BadSlug) => {
            return (StatusCode::NOT_FOUND, Html(pages::not_found())).into_response();
        }
        Err(e) => return store_error(e),
    };
    let body = match app.store.get_content(&project, &slug).await {
        Ok(b) => b,
        Err(e) => return store_error(e),
    };
    let kind = view_kind(&meta.content_type, &meta.slug);
    let title = meta.title.clone();
    let slug_owned = meta.slug.clone();
    let rendered =
        match tokio::task::spawn_blocking(move || pdf::render(kind, &title, &slug_owned, &body))
            .await
        {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(e)) => {
                warn!("pdf export failed: {e}");
                return (StatusCode::BAD_REQUEST, e).into_response();
            }
            Err(e) => {
                warn!("pdf export join failed: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "pdf export failed").into_response();
            }
        };
    let filename = pdf::filename(&meta.slug);
    let mut res = Response::new(Body::from(rendered));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            filename.replace('"', "")
        ))
        .unwrap_or(HeaderValue::from_static(
            "attachment; filename=\"document.pdf\"",
        )),
    );
    res.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    res
}

async fn raw_doc_p(
    State(app): State<App>,
    Authed: Authed,
    Path((project, slug)): Path<(String, String)>,
) -> Response {
    raw_body(app, project, slug).await
}

async fn raw_body(app: App, project: String, slug: String) -> Response {
    let meta = match app.store.get_meta(&project, &slug).await {
        Ok(m) => m,
        Err(StoreError::NotFound | StoreError::BadSlug) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => return store_error(e),
    };
    let body = match app.store.get_content(&project, &slug).await {
        Ok(b) => b,
        Err(e) => return store_error(e),
    };
    let mut res = Response::new(Body::from(body));
    let ct = sanitize_content_type(&meta.content_type);
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&ct).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "inline; filename=\"{}\"",
            meta.slug.replace('"', "")
        ))
        .unwrap_or(HeaderValue::from_static("inline")),
    );
    res.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    res
}

async fn delete_doc(State(app): State<App>, Authed: Authed, Path(slug): Path<String>) -> Response {
    delete_and_redirect(app, DEFAULT_PROJECT, &slug).await
}

async fn delete_doc_p(
    State(app): State<App>,
    Authed: Authed,
    Path((project, slug)): Path<(String, String)>,
) -> Response {
    delete_and_redirect(app, &project, &slug).await
}

async fn delete_and_redirect(app: App, project: &str, slug: &str) -> Response {
    match app.store.delete(project, slug).await {
        Ok(()) => Redirect::to(&format!("/?project={project}")).into_response(),
        Err(StoreError::NotFound | StoreError::BadSlug) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => store_error(e),
    }
}

async fn delete_doc_api(
    State(app): State<App>,
    Authed: Authed,
    Path(slug): Path<String>,
) -> Response {
    delete_no_content(app, DEFAULT_PROJECT, &slug).await
}

async fn delete_doc_api_p(
    State(app): State<App>,
    Authed: Authed,
    Path((project, slug)): Path<(String, String)>,
) -> Response {
    delete_no_content(app, &project, &slug).await
}

async fn delete_no_content(app: App, project: &str, slug: &str) -> Response {
    match app.store.delete(project, slug).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StoreError::NotFound | StoreError::BadSlug) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => store_error(e),
    }
}

async fn api_list(State(app): State<App>, Authed: Authed) -> Response {
    match app.store.list().await {
        Ok(docs) => Json(docs).into_response(),
        Err(e) => store_error(e),
    }
}

async fn api_get(State(app): State<App>, Authed: Authed, Path(slug): Path<String>) -> Response {
    json_meta(app, DEFAULT_PROJECT, &slug).await
}

async fn api_get_p(
    State(app): State<App>,
    Authed: Authed,
    Path((project, slug)): Path<(String, String)>,
) -> Response {
    json_meta(app, &project, &slug).await
}

async fn json_meta(app: App, project: &str, slug: &str) -> Response {
    match app.store.get_meta(project, slug).await {
        Ok(meta) => Json(meta).into_response(),
        Err(StoreError::NotFound | StoreError::BadSlug) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => store_error(e),
    }
}

fn authed(app: &App, headers: &HeaderMap) -> bool {
    extract_from_headers(headers).is_some_and(|got| app.token.matches(&got))
}

fn redirect_to_login(next: &str) -> Response {
    let next = safe_next(Some(next));
    if next == "/" {
        return Redirect::to("/login").into_response();
    }
    Redirect::to(&format!("/login?next={}", urlenc(&next))).into_response()
}

fn safe_next(next: Option<&str>) -> String {
    let Some(s) = next.map(str::trim).filter(|s| !s.is_empty()) else {
        return "/".into();
    };
    if !s.starts_with('/')
        || s.starts_with("//")
        || s.contains('\\')
        || s.contains('\n')
        || s.contains('\r')
    {
        return "/".into();
    }
    if s == "/login" || s.starts_with("/login?") {
        return "/".into();
    }
    s.to_string()
}

fn urlenc(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            c => out.push_str(&format!("%{c:02X}")),
        }
    }
    out
}

fn set_cookie_and_redirect(app: &App, headers: &HeaderMap, token: &str, to: &str) -> Response {
    let mut cookie = Cookie::new(Token::cookie_name(), token.to_string());
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(COOKIE_MAX_AGE);
    cookie.set_expires(OffsetDateTime::now_utc() + COOKIE_MAX_AGE);
    let https = app
        .public_base
        .as_deref()
        .is_some_and(|u| u.starts_with("https://"))
        || headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            == Some("https");
    if https {
        cookie.set_secure(true);
    }
    let mut res = Redirect::to(to).into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie.to_string()).expect("cookie"),
    );
    res
}

fn origin(app: &App, headers: &HeaderMap) -> String {
    if let Some(base) = &app.public_base {
        return base.trim_end_matches('/').to_string();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    format!("{proto}://{host}")
}

fn public_doc_url(app: &App, headers: &HeaderMap, meta: &store::Meta) -> String {
    format!("{}{}", origin(app, headers), meta.href())
}

fn project_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-project")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_PROJECT)
        .to_string()
}

fn store_error(e: StoreError) -> Response {
    match e {
        StoreError::BadSlug => (StatusCode::BAD_REQUEST, "invalid slug").into_response(),
        StoreError::NotFound => StatusCode::NOT_FOUND.into_response(),
        StoreError::Io(err) => {
            warn!("store io: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "store error").into_response()
        }
        StoreError::Meta(err) => {
            warn!("store meta: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "store error").into_response()
        }
    }
}

fn new_slug() -> String {
    let date = OffsetDateTime::now_utc()
        .date()
        .to_string()
        .replace('-', "");
    format!("{date}-{}", nanoid::nanoid!(8))
}

fn safe_filename(name: &str) -> Option<String> {
    let base = std::path::Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())?;
    if Store::validate_slug(base).is_ok() {
        Some(base.to_string())
    } else {
        None
    }
}

fn wants_html(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"))
}

fn sanitize_content_type(ct: &str) -> String {
    let base = ct.split(';').next().unwrap_or(ct).trim();
    if base.eq_ignore_ascii_case("text/html")
        || base.eq_ignore_ascii_case("image/svg+xml")
        || base.eq_ignore_ascii_case("text/javascript")
        || base.eq_ignore_ascii_case("application/javascript")
    {
        return "text/plain; charset=utf-8".into();
    }
    if base.starts_with("text/") && !base.contains("charset") {
        return format!("{base}; charset=utf-8");
    }
    base.to_string()
}

// unix-only OpenOptions::mode
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[cfg(not(unix))]
trait ModeExt {
    fn mode(&mut self, _: u32) -> &mut Self {
        self
    }
}
#[cfg(not(unix))]
impl ModeExt for std::fs::OpenOptions {}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_app(dir: PathBuf) -> Router {
        router(App {
            store: Store::new(dir),
            token: Token("test-token-16chars".into()),
            max_bytes: 64 * 1024,
            public_base: Some("http://gist.test".into()),
        })
    }

    async fn body_string(res: Response) -> String {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn agent_card_is_public() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/agent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let json = body_string(res).await;
        assert!(json.contains("GIST_TOKEN"));
        assert!(json.contains("/d/{project}/{slug}"));
        assert!(!json.contains("test-token-16chars"));
    }

    #[tokio::test]
    async fn basic_auth_put() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir.clone());
        use base64::Engine;
        let basic = base64::engine::general_purpose::STANDARD.encode("gist:test-token-16chars");
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("PUT")
                    .uri("/d/via-basic.md")
                    .header("authorization", format!("Basic {basic}"))
                    .header("content-type", "text/markdown")
                    .body(Body::from("# via basic"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn login_page_is_a_form_not_a_basic_prompt() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get(header::WWW_AUTHENTICATE).is_none());
        let html = body_string(res).await;
        assert!(html.contains("method=\"post\""));
        assert!(html.contains("name=\"token\""));
        assert!(html.contains("Sign in"));
        assert!(html.contains("/static/theme.js"));
        assert!(html.contains("id=\"color-scheme\""));
    }

    #[tokio::test]
    async fn doc_js_zooms_mermaid() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/static/doc.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let js = body_string(res).await;
        assert!(js.contains("enhanceMermaid"));
        assert!(js.contains("mermaid-viewport"));
        assert!(js.contains("is-expanded"));
        assert!(js.contains("wireMermaidZoom"));
    }

    #[tokio::test]
    async fn theme_js_is_public() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/static/theme.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let js = body_string(res).await;
        assert!(js.contains("gist-theme"));
        assert!(js.contains("data-theme"));
    }

    #[tokio::test]
    async fn unauth_browser_is_sent_to_login() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(res.status().is_redirection());
        assert!(res.headers().get(header::WWW_AUTHENTICATE).is_none());
        let loc = res
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(loc, "/login");
    }

    #[tokio::test]
    async fn login_sets_cookie_and_honors_next() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("token=test-token-16chars&next=/d/notes.md"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(res.status().is_redirection());
        let loc = res
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(loc, "/d/notes.md");
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(cookie.contains("gist_token="));
        assert!(cookie.contains("HttpOnly"));
    }

    #[tokio::test]
    async fn login_rejects_open_redirect() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "token=test-token-16chars&next=https://evil.example/",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let loc = res
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(loc, "/");
    }

    #[tokio::test]
    async fn unauth_put_rejected() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("PUT")
                    .uri("/d/notes.md")
                    .header("content-type", "text/markdown")
                    .body(Body::from("# hi"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn put_and_list() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir.clone());
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PUT")
                    .uri("/d/notes.md")
                    .header("authorization", "Bearer test-token-16chars")
                    .header("content-type", "text/markdown")
                    .body(Body::from("# Standup\n\nhello **world**"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let json = body_string(res).await;
        assert!(json.contains("standup.md") || json.contains("notes.md"));
        assert!(json.contains("/d/inbox/notes.md"));

        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/d/notes.md")
                    .header("authorization", "Bearer test-token-16chars")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let html = body_string(res).await;
        assert!(html.contains("<strong>world</strong>") || html.contains("<p>hello"));
        assert!(html.contains("Standup"));
        assert!(html.contains("/d/inbox/notes.md/pdf"));

        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/docs")
                    .header("authorization", "Bearer test-token-16chars")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn export_markdown_pdf() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir.clone());
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PUT")
                    .uri("/d/design.md")
                    .header("authorization", "Bearer test-token-16chars")
                    .header("content-type", "text/markdown")
                    .body(Body::from("# Design\n\nHello.\n\n## Plan\n\nWorld.\n"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/d/design.md/pdf")
                    .header("authorization", "Bearer test-token-16chars")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/pdf")
        );
        let disp = res
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(disp.contains("design.pdf"));
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 500);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn put_into_project() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir.clone());
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PUT")
                    .uri("/d/gist/design.md")
                    .header("authorization", "Bearer test-token-16chars")
                    .header("content-type", "text/markdown")
                    .body(Body::from("# Design\n\nin a project"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let json = body_string(res).await;
        assert!(json.contains("\"project\":\"gist\""));
        assert!(json.contains("/d/gist/design.md"));

        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/d/gist/design.md")
                    .header("authorization", "Bearer test-token-16chars")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let html = body_string(res).await;
        assert!(html.contains("in a project"));
        assert!(html.contains("gist"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn path_traversal_rejected() {
        let dir = std::env::temp_dir().join(format!("gist-test-{}", nanoid::nanoid!(6)));
        let app = test_app(dir);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("PUT")
                    .uri("/d/..%2Fetc")
                    .header("authorization", "Bearer test-token-16chars")
                    .body(Body::from("nope"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::NOT_FOUND,
            "{}",
            res.status()
        );
    }
}
