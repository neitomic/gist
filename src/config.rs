use std::{
    fs,
    io::{ErrorKind, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub const DEFAULT_BIND: &str = "127.0.0.1:8787";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Advertised origin (`gist env`, remote agents).
    pub url: String,
    pub token: String,
    /// Loopback origin for `gist put` / `gist list` on this machine.
    pub local_url: String,
}

impl Config {
    pub fn is_ready(&self) -> bool {
        self.token.len() >= 16 && (!self.url.is_empty() || !self.local_url.is_empty())
    }

    /// Base URL for this machine's CLI (`put` / `list`).
    pub fn cli_url(&self) -> &str {
        if !self.local_url.is_empty() {
            &self.local_url
        } else {
            &self.url
        }
    }

    /// Base URL printed by `gist env` for a remote agent.
    pub fn env_url(&self) -> &str {
        if !self.url.is_empty() {
            &self.url
        } else {
            &self.local_url
        }
    }
}

pub fn path() -> PathBuf {
    if let Ok(p) = std::env::var("GIST_CONFIG") {
        return PathBuf::from(p);
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("gist/config");
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/gist/config")
}

pub fn load() -> Config {
    let mut cfg = load_file(&path()).unwrap_or_default();
    if let Ok(url) = std::env::var("GIST_URL") {
        let url = url.trim().to_string();
        if !url.is_empty() {
            cfg.url = url;
        }
    }
    if let Ok(local) = std::env::var("GIST_LOCAL_URL") {
        let local = local.trim().to_string();
        if !local.is_empty() {
            cfg.local_url = local;
        }
    }
    if let Ok(token) = std::env::var("GIST_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            cfg.token = token;
        }
    }
    cfg.url = trim_slash(&cfg.url);
    cfg.local_url = trim_slash(&cfg.local_url);
    cfg
}

pub fn load_file(path: &Path) -> Option<Config> {
    let text = fs::read_to_string(path).ok()?;
    Some(parse(&text))
}

pub fn parse(text: &str) -> Config {
    let mut cfg = Config::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"').to_string();
        match k.trim() {
            "url" | "GIST_URL" => cfg.url = v,
            "local_url" | "GIST_LOCAL_URL" => cfg.local_url = v,
            "token" | "GIST_TOKEN" => cfg.token = v,
            _ => {}
        }
    }
    cfg
}

pub fn save(cfg: &Config) -> std::io::Result<PathBuf> {
    let path = path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let url = trim_slash(&cfg.url);
    let local = trim_slash(&cfg.local_url);
    let mut body = format!(
        "# gist agent credentials — mode 0600, do not commit\nurl={url}\ntoken={}\n",
        cfg.token
    );
    if !local.is_empty() && local != url {
        body.push_str(&format!("local_url={local}\n"));
    }
    write_private(&path, body.as_bytes())?;
    Ok(path)
}

pub fn client_url(bind: &str, public: Option<&str>) -> String {
    if let Some(url) = public {
        return trim_slash(url);
    }
    loopback_url(bind)
}

pub fn loopback_url(bind: &str) -> String {
    let addr = bind.parse::<SocketAddr>().ok();
    match addr {
        Some(a) if a.ip().is_unspecified() || a.ip().is_loopback() => {
            format!("http://127.0.0.1:{}", a.port())
        }
        Some(a) => format!("http://{a}"),
        None => format!("http://{bind}"),
    }
}

/// Merge listen addresses into an existing config without dropping a remote `url`.
pub fn apply_listen(mut cfg: Config, bind: &str, public: Option<&str>, token: &str) -> Config {
    cfg.token = token.to_string();
    let local = loopback_url(bind);
    let advertised = client_url(bind, public);
    if bind_is_local(bind) {
        cfg.local_url = local;
    }
    if public.is_some() || cfg.url.is_empty() || is_loopback_http(&cfg.url) {
        cfg.url = advertised;
    }
    cfg
}

fn bind_is_local(bind: &str) -> bool {
    match bind.parse::<SocketAddr>() {
        Ok(a) => a.ip().is_loopback() || a.ip().is_unspecified(),
        Err(_) => bind.contains("127.0.0.1") || bind.starts_with("localhost"),
    }
}

fn is_loopback_http(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    u.contains("://127.0.0.1") || u.contains("://localhost") || u.contains("://[::1]")
}

fn trim_slash(s: &str) -> String {
    s.trim().trim_end_matches('/').to_string()
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("config.tmp");
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        match opts.open(&tmp) {
            Ok(mut f) => f.write_all(bytes)?,
            Err(e) if e.kind() == ErrorKind::PermissionDenied => return Err(e),
            Err(e) => return Err(e),
        }
    }
    fs::rename(&tmp, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_values() {
        let cfg = parse("url=http://127.0.0.1:8787\ntoken=abcdefghijklmnopqrst\n");
        assert_eq!(cfg.url, "http://127.0.0.1:8787");
        assert_eq!(cfg.token, "abcdefghijklmnopqrst");
        assert!(cfg.is_ready());
    }

    #[test]
    fn parse_local_url() {
        let cfg = parse(
            "url=https://gist.example.com\ntoken=abcdefghijklmnopqrst\nlocal_url=http://127.0.0.1:8787\n",
        );
        assert_eq!(cfg.cli_url(), "http://127.0.0.1:8787");
        assert_eq!(cfg.env_url(), "https://gist.example.com");
    }

    #[test]
    fn client_url_rewrites_wildcard_bind() {
        assert_eq!(client_url("0.0.0.0:8787", None), "http://127.0.0.1:8787");
        assert_eq!(
            client_url("127.0.0.1:8787", Some("https://gist.example.com/")),
            "https://gist.example.com"
        );
    }

    #[test]
    fn apply_listen_keeps_remote_url() {
        let existing = Config {
            url: "https://gist.example.com".into(),
            token: "old-token-16chars".into(),
            local_url: String::new(),
        };
        let cfg = apply_listen(existing, "127.0.0.1:8787", None, "new-token-16chars");
        assert_eq!(cfg.url, "https://gist.example.com");
        assert_eq!(cfg.local_url, "http://127.0.0.1:8787");
        assert_eq!(cfg.token, "new-token-16chars");
        assert_eq!(cfg.cli_url(), "http://127.0.0.1:8787");
        assert_eq!(cfg.env_url(), "https://gist.example.com");
    }

    #[test]
    fn apply_listen_fills_empty_url() {
        let cfg = apply_listen(
            Config::default(),
            "127.0.0.1:8787",
            None,
            "abcdefghijklmnopqrst",
        );
        assert_eq!(cfg.url, "http://127.0.0.1:8787");
        assert_eq!(cfg.local_url, "http://127.0.0.1:8787");
    }

    #[test]
    fn apply_listen_public_url_wins_for_env() {
        let cfg = apply_listen(
            Config::default(),
            "127.0.0.1:8787",
            Some("https://gist.example.com/"),
            "abcdefghijklmnopqrst",
        );
        assert_eq!(cfg.url, "https://gist.example.com");
        assert_eq!(cfg.local_url, "http://127.0.0.1:8787");
    }
}
