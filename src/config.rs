use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub url: String,
    pub token: String,
}

impl Config {
    pub fn is_ready(&self) -> bool {
        !self.url.is_empty() && self.token.len() >= 16
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
    if let Ok(token) = std::env::var("GIST_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            cfg.token = token;
        }
    }
    cfg.url = trim_slash(&cfg.url);
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
    let body = format!(
        "# gist agent credentials — mode 0600, do not commit\nurl={}\ntoken={}\n",
        trim_slash(&cfg.url),
        cfg.token
    );
    write_private(&path, body.as_bytes())?;
    Ok(path)
}

pub fn client_url(bind: &str, public: Option<&str>) -> String {
    if let Some(url) = public {
        return trim_slash(url);
    }
    let addr = bind.parse::<std::net::SocketAddr>().ok();
    match addr {
        Some(a) if a.ip().is_unspecified() => format!("http://127.0.0.1:{}", a.port()),
        Some(a) => format!("http://{a}"),
        None => format!("http://{bind}"),
    }
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
    fn client_url_rewrites_wildcard_bind() {
        assert_eq!(client_url("0.0.0.0:8787", None), "http://127.0.0.1:8787");
        assert_eq!(
            client_url("127.0.0.1:8787", Some("https://gist.example.com/")),
            "https://gist.example.com"
        );
    }
}
