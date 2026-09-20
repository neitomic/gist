//! `gist agents` — write the upload contract where coding agents look for it.
//!
//! Claude, Codex and Grok all load skills from `<root>/.<agent>/skills/<name>/SKILL.md`
//! with the same frontmatter, so one file serves all three. gist owns that file
//! and never edits an agent's own instructions, such as `AGENTS.md`.

use std::path::{Path, PathBuf};

const FRONTMATTER: &str = "\
---
name: gist
description: >
  Share files and write-ups with the user through their private gist inbox.
  Use when you have a document to hand over, when the user says \"put this in
  gist\", \"drop this\", \"share the notes\", or when a markdown/html/json result
  should be read in the browser instead of pasted into chat.
---

";

const BODY: &str = include_str!("agent_skill.md");

#[derive(Copy, Clone, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum Target {
    Claude,
    Codex,
    Grok,
}

impl Target {
    pub const ALL: [Target; 3] = [Target::Claude, Target::Codex, Target::Grok];

    /// Also the agent's dot-directory: `.claude`, `.codex`, `.grok`.
    pub fn name(self) -> &'static str {
        match self {
            Target::Claude => "claude",
            Target::Codex => "codex",
            Target::Grok => "grok",
        }
    }

    /// `~/.codex/skills/gist/SKILL.md`, or the same path under a repository.
    pub fn path(self, root: &Path) -> PathBuf {
        root.join(format!(".{}/skills/gist/SKILL.md", self.name()))
    }
}

pub fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| "HOME is not set".to_string())
}

pub fn skill_file() -> String {
    format!("{FRONTMATTER}{}", BODY.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_file_has_frontmatter_and_body() {
        let s = skill_file();
        assert!(s.starts_with("---\nname: gist\n"));
        assert!(s.contains("description:"));
        assert!(s.contains("gist put"));
        // One trailing newline at most; the file is written verbatim.
        assert!(!s.ends_with("\n\n"));
    }

    #[test]
    fn every_agent_gets_its_own_skill_file() {
        let root = Path::new("/r");
        assert_eq!(
            Target::Claude.path(root),
            Path::new("/r/.claude/skills/gist/SKILL.md")
        );
        assert_eq!(
            Target::Codex.path(root),
            Path::new("/r/.codex/skills/gist/SKILL.md")
        );
        assert_eq!(
            Target::Grok.path(root),
            Path::new("/r/.grok/skills/gist/SKILL.md")
        );
    }

    #[test]
    fn nothing_targets_a_shared_instructions_file() {
        // gist owns only its own skill file; AGENTS.md and CLAUDE.md are the
        // user's and must never be written by this command.
        for t in Target::ALL {
            let p = t.path(Path::new("/r"));
            assert!(p.ends_with("skills/gist/SKILL.md"), "{p:?}");
            let s = p.to_string_lossy();
            assert!(!s.contains("AGENTS.md") && !s.contains("CLAUDE.md"), "{s}");
        }
    }
}
