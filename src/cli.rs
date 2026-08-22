use crate::config::{self, Config};
use crate::store::{guess_content_type, Store};
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(clap::Parser, Debug)]
#[command(
    name = "gist",
    about = "Token-gated document drop for agents",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run the inbox server (default)
    Serve,
    /// Upload a file (reads ~/.config/gist/config)
    Put {
        /// File to upload
        file: PathBuf,
        /// Project to group under (default: $GIST_PROJECT, git root name, or inbox)
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Document slug (default: file name)
        #[arg(long, short)]
        slug: Option<String>,
        /// Title shown in the inbox
        #[arg(long, short)]
        title: Option<String>,
    },
    /// List documents as JSON
    List,
    /// Print export lines for GIST_URL and GIST_TOKEN
    Env,
}

pub fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Serve => unreachable!("serve is handled in main"),
        Command::Put {
            file,
            project,
            slug,
            title,
        } => put(file, project, slug, title),
        Command::List => list(),
        Command::Env => print_env(),
    }
}

fn require_config() -> Result<Config, String> {
    let cfg = config::load();
    if !cfg.is_ready() {
        return Err(format!(
            "missing credentials. set GIST_URL and GIST_TOKEN, or start the server once so it writes {}",
            config::path().display()
        ));
    }
    Ok(cfg)
}

fn put(
    file: PathBuf,
    project: Option<String>,
    slug: Option<String>,
    title: Option<String>,
) -> Result<(), String> {
    let cfg = require_config()?;
    let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let name = file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("upload");
    let slug = slug.unwrap_or_else(|| name.to_string());
    let project = project.unwrap_or_else(infer_project);
    Store::validate_project(&project).map_err(|_| format!("invalid project '{project}'"))?;
    Store::validate_slug(&slug).map_err(|_| {
        format!("invalid slug '{slug}' (use letters, digits, '.', '_' or '-', max 128)")
    })?;
    let ct = guess_content_type(&slug, None);
    let mut req = ureq::put(&format!("{}/d/{project}/{slug}", cfg.url))
        .set("Authorization", &format!("Bearer {}", cfg.token))
        .set("Content-Type", &ct);
    if let Some(title) = title.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        req = req.set("X-Title", title);
    }
    let resp = req
        .send_bytes(&bytes)
        .map_err(|e| format!("upload failed: {e}"))?;
    let status = resp.status();
    let mut body = String::new();
    resp.into_reader()
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("server {status}: {body}"));
    }
    println!("{body}");
    Ok(())
}

fn list() -> Result<(), String> {
    let cfg = require_config()?;
    let resp = ureq::get(&format!("{}/api/docs", cfg.url))
        .set("Authorization", &format!("Bearer {}", cfg.token))
        .call()
        .map_err(|e| format!("list failed: {e}"))?;
    let mut body = String::new();
    resp.into_reader()
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    println!("{body}");
    Ok(())
}

fn print_env() -> Result<(), String> {
    let cfg = require_config()?;
    println!("export GIST_URL={}", shell_single(&cfg.url));
    println!("export GIST_TOKEN={}", shell_single(&cfg.token));
    let _ = writeln!(std::io::stderr(), "# from {}", config::path().display());
    Ok(())
}

fn infer_project() -> String {
    if let Ok(p) = std::env::var("GIST_PROJECT") {
        let p = p.trim();
        if !p.is_empty() && Store::validate_project(p).is_ok() {
            return p.to_string();
        }
    }
    if let Ok(out) = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if out.status.success() {
            let root = String::from_utf8_lossy(&out.stdout);
            let root = root.trim();
            if let Some(name) = std::path::Path::new(root).file_name() {
                let name = name.to_string_lossy();
                if Store::validate_project(&name).is_ok() {
                    return name.into_owned();
                }
            }
        }
    }
    crate::store::DEFAULT_PROJECT.to_string()
}

fn shell_single(s: &str) -> String {
    format!("'{}'", s.replace('\'', r#"'"'"'"#))
}
