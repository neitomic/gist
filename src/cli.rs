use crate::config::{self, Config};
use crate::store::{content_type_from_filename, Store};
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
    pub command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run the inbox server
    Serve {
        /// Overwrite this machine's client credentials without asking
        #[arg(long, short = 'y')]
        yes: bool,
    },
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
    /// Install a macOS launchd service and point this CLI at it
    Install,
    /// Stop and remove the macOS launchd service
    Uninstall,
    /// Write the gist instructions where coding agents will read them
    Agents {
        /// Which agents to set up (default: all)
        #[arg(value_enum)]
        targets: Vec<crate::agents::Target>,
        /// Write into this repository instead of the home directory
        #[arg(long, short)]
        project: bool,
    },
}

pub fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Serve { .. } => unreachable!("serve is handled in main"),
        Command::Put {
            file,
            project,
            slug,
            title,
        } => put(file, project, slug, title),
        Command::List => list(),
        Command::Env => print_env(),
        Command::Install => crate::service::install(),
        Command::Uninstall => crate::service::uninstall(),
        Command::Agents { targets, project } => install_agents(&targets, project),
    }
}

fn install_agents(targets: &[crate::agents::Target], project: bool) -> Result<(), String> {
    use crate::agents::Target;
    let chosen: Vec<Target> = if targets.is_empty() {
        Target::ALL.to_vec()
    } else {
        let mut v = targets.to_vec();
        v.dedup();
        v
    };
    // Machine-wide by default: the inbox is per-machine, and a project copy
    // would be committed into an unrelated repository.
    let root = if project {
        project_root()?
    } else {
        crate::agents::home_dir()?
    };
    let contents = crate::agents::skill_file();
    for t in chosen {
        let path = t.path(&root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, &contents).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("{:<7} {}", t.name(), path.display());
    }
    Ok(())
}

/// Skills belong at the top of the repository, not wherever the command
/// happened to be run.
fn project_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("current directory: {e}"))?;
    Ok(git_toplevel().unwrap_or(cwd))
}

fn git_toplevel() -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if root.is_empty() {
        return None;
    }
    Some(PathBuf::from(root))
}

fn require_config() -> Result<Config, String> {
    let cfg = config::load();
    if !cfg.is_ready() {
        return Err(format!(
            "missing credentials. set GIST_URL and GIST_TOKEN, run `gist install` on macOS, or start the server once so it writes {}",
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
    let ct = content_type_from_filename(name);
    let mut req = ureq::put(&format!("{}/d/{project}/{slug}", cfg.cli_url()))
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
    let resp = ureq::get(&format!("{}/api/docs", cfg.cli_url()))
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
    println!("export GIST_URL={}", shell_single(cfg.env_url()));
    println!("export GIST_TOKEN={}", shell_single(&cfg.token));
    let _ = writeln!(std::io::stderr(), "# from {}", config::path().display());
    if cfg.cli_url() != cfg.env_url() {
        let _ = writeln!(
            std::io::stderr(),
            "# this machine's gist put/list use {}",
            cfg.cli_url()
        );
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn bare_gist_does_not_start_the_server() {
        // No subcommand must show usage, never start an implicit `serve`.
        let err = Cli::try_parse_from(["gist"]).unwrap_err();
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
        let shown = err.to_string();
        assert!(shown.contains("serve"), "help should list serve: {shown}");
        assert!(shown.contains("put"), "help should list put: {shown}");
    }

    #[test]
    fn serving_is_explicit() {
        let cli = Cli::try_parse_from(["gist", "serve"]).unwrap();
        assert!(matches!(cli.command, Command::Serve { yes: false }));
    }

    #[test]
    fn cli_subcommands_still_parse() {
        let cli = Cli::try_parse_from(["gist", "put", "notes.md"]).unwrap();
        assert!(matches!(cli.command, Command::Put { .. }));
    }
}
