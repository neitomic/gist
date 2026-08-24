pub const LABEL: &str = "xyz.neitomic.gist";

pub fn install() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::install()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("gist install is only supported on macOS (launchd)".into())
    }
}

pub fn uninstall() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::uninstall()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("gist uninstall is only supported on macOS (launchd)".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{plist_body, LABEL};
    use crate::config::{self, DEFAULT_BIND};
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::Duration;

    pub fn install() -> Result<(), String> {
        let home = home_dir()?;
        let data = data_dir(&home);
        let bind = std::env::var("GIST_BIND").unwrap_or_else(|_| DEFAULT_BIND.into());
        let bin_dir = data.join("bin");
        let bin = bin_dir.join("gist");
        let logs = home.join("Library/Logs");
        let stdout_log = logs.join("gist.log");
        let stderr_log = logs.join("gist.err.log");
        let agents = home.join("Library/LaunchAgents");
        let plist_path = agents.join(format!("{LABEL}.plist"));

        fs::create_dir_all(&bin_dir).map_err(|e| format!("{}: {e}", bin_dir.display()))?;
        fs::create_dir_all(&data).map_err(|e| format!("{}: {e}", data.display()))?;
        fs::create_dir_all(&logs).map_err(|e| format!("{}: {e}", logs.display()))?;
        fs::create_dir_all(&agents).map_err(|e| format!("{}: {e}", agents.display()))?;

        let uid = user_id()?;
        let domain = format!("gui/{uid}");
        let target = format!("{domain}/{LABEL}");
        let _ = launchctl(&["bootout", &target]);

        install_binary(&bin)?;
        maybe_install_path_bin(&bin)?;

        let token = resolve_token(&data)?;
        let plist = plist_body(
            bin.to_str().ok_or("binary path is not UTF-8")?,
            &bind,
            data.to_str().ok_or("data path is not UTF-8")?,
            stdout_log.to_str().ok_or("log path is not UTF-8")?,
            stderr_log.to_str().ok_or("log path is not UTF-8")?,
        );
        write_plist(&plist_path, &plist)?;

        let plist_str = plist_path.to_str().ok_or("plist path is not UTF-8")?;
        launchctl(&["bootstrap", &domain, plist_str])?;
        let _ = launchctl(&["enable", &target]);
        let _ = launchctl(&["kickstart", "-k", &target]);

        let local = config::loopback_url(&bind);
        wait_healthy(&local)?;

        let mut cfg = config::load_file(&config::path()).unwrap_or_default();
        cfg = config::apply_listen(cfg, &bind, None, &token);
        let path =
            config::save(&cfg).map_err(|e| format!("write {}: {e}", config::path().display()))?;

        println!("installed {LABEL}");
        println!("  browse    {}", cfg.cli_url());
        println!("  data      {}", data.display());
        println!("  binary    {}", bin.display());
        println!(
            "  logs      {}  {}",
            stdout_log.display(),
            stderr_log.display()
        );
        println!("  config    {}", path.display());
        println!("  launchctl print {target}");
        if cfg.env_url() != cfg.cli_url() {
            println!(
                "  this machine's gist put/list use {}  (gist env still prints {})",
                cfg.cli_url(),
                cfg.env_url()
            );
        }
        Ok(())
    }

    pub fn uninstall() -> Result<(), String> {
        let home = home_dir()?;
        let plist_path = home
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist"));
        let uid = user_id()?;
        let target = format!("gui/{uid}/{LABEL}");
        match launchctl(&["bootout", &target]) {
            Ok(_) => println!("stopped {LABEL}"),
            Err(e) => eprintln!("gist: {LABEL} was not loaded ({e})"),
        }
        match fs::remove_file(&plist_path) {
            Ok(()) => println!("removed {}", plist_path.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", plist_path.display())),
        }
        let data = data_dir(&home);
        println!(
            "data kept at {}  (delete that folder to wipe documents)",
            data.display()
        );
        Ok(())
    }

    fn home_dir() -> Result<PathBuf, String> {
        std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| "HOME is not set".into())
    }

    fn data_dir(home: &Path) -> PathBuf {
        if let Ok(p) = std::env::var("GIST_DATA") {
            let p = p.trim();
            if !p.is_empty() {
                return PathBuf::from(p);
            }
        }
        home.join("Library/Application Support/gist")
    }

    fn user_id() -> Result<String, String> {
        let out = Command::new("id")
            .arg("-u")
            .output()
            .map_err(|e| format!("id -u: {e}"))?;
        if !out.status.success() {
            return Err("id -u failed".into());
        }
        let uid = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if uid.is_empty() {
            return Err("id -u returned empty".into());
        }
        Ok(uid)
    }

    fn launchctl(args: &[&str]) -> Result<String, String> {
        let out = Command::new("launchctl")
            .args(args)
            .output()
            .map_err(|e| format!("launchctl: {e}"))?;
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !out.status.success() {
            let msg = if stderr.is_empty() { stdout } else { stderr };
            return Err(format!("launchctl {}: {msg}", args.join(" ")));
        }
        Ok(stdout)
    }

    fn install_binary(dest: &Path) -> Result<(), String> {
        let src = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
        if src == dest {
            return Ok(());
        }
        fs::copy(&src, dest)
            .map_err(|e| format!("copy {} → {}: {e}", src.display(), dest.display()))?;
        fs::set_permissions(dest, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", dest.display()))?;
        Ok(())
    }

    fn maybe_install_path_bin(service_bin: &Path) -> Result<(), String> {
        let home = home_dir()?;
        if home.join(".cargo/bin/gist").exists() {
            return Ok(());
        }
        let local_bin_dir = home.join(".local/bin");
        fs::create_dir_all(&local_bin_dir)
            .map_err(|e| format!("{}: {e}", local_bin_dir.display()))?;
        let dest = local_bin_dir.join("gist");
        if dest == service_bin {
            return Ok(());
        }
        fs::copy(service_bin, &dest)
            .map_err(|e| format!("copy {} → {}: {e}", service_bin.display(), dest.display()))?;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", dest.display()))?;
        if let Ok(path) = std::env::var("PATH") {
            let listed = path.split(':').any(|p| Path::new(p) == local_bin_dir);
            if !listed {
                eprintln!(
                    "gist: add {} to PATH so `gist put` is on this machine",
                    local_bin_dir.display()
                );
            }
        }
        Ok(())
    }

    fn resolve_token(data: &Path) -> Result<String, String> {
        if let Ok(t) = std::env::var("GIST_TOKEN") {
            let t = t.trim().to_string();
            if t.len() < 16 {
                return Err("GIST_TOKEN must be at least 16 characters".into());
            }
            return Ok(t);
        }
        if let Some(cfg) = config::load_file(&config::path()) {
            if cfg.token.len() >= 16 {
                return Ok(cfg.token);
            }
        }
        let path = data.join(".token");
        if let Ok(existing) = fs::read_to_string(&path) {
            let t = existing.trim().to_string();
            if t.len() >= 16 {
                return Ok(t);
            }
        }
        let generated = format!("gst_{}", nanoid::nanoid!(32));
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        writeln!(f, "{generated}").map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(generated)
    }

    fn write_plist(path: &Path, body: &str) -> Result<(), String> {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o644);
        opts.open(path)
            .and_then(|mut f| f.write_all(body.as_bytes()))
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    fn wait_healthy(base: &str) -> Result<(), String> {
        let url = format!("{base}/health");
        for _ in 0..40 {
            if let Ok(resp) = ureq::get(&url).timeout(Duration::from_millis(400)).call() {
                if resp.status() == 200 {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(125));
        }
        Err(format!(
            "service did not become healthy at {url}. check ~/Library/Logs/gist.err.log"
        ))
    }
}

pub fn plist_body(bin: &str, bind: &str, data: &str, stdout: &str, stderr: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{bin}</string>
    <string>serve</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>GIST_BIND</key>
    <string>{bind}</string>
    <key>GIST_DATA</key>
    <string>{data}</string>
  </dict>
  <key>WorkingDirectory</key>
  <string>{data}</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{stdout}</string>
  <key>StandardErrorPath</key>
  <string>{stderr}</string>
</dict>
</plist>
"#,
        label = xml_escape(LABEL),
        bin = xml_escape(bin),
        bind = xml_escape(bind),
        data = xml_escape(data),
        stdout = xml_escape(stdout),
        stderr = xml_escape(stderr),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_escapes_and_contains_serve() {
        let xml = plist_body(
            "/Users/me/Library/Application Support/gist/bin/gist",
            "127.0.0.1:8787",
            "/Users/me/Library/Application Support/gist",
            "/Users/me/Library/Logs/gist.log",
            "/Users/me/Library/Logs/gist.err.log",
        );
        assert!(xml.contains("<string>xyz.neitomic.gist</string>"));
        assert!(xml.contains("<string>serve</string>"));
        assert!(xml.contains("Application Support/gist/bin/gist"));
        assert!(xml.contains("GIST_BIND"));
        assert!(!xml.contains("GIST_TOKEN"));
        assert!(xml.contains("<key>KeepAlive</key>"));
    }
}
