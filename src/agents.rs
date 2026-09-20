//! `gist agents` — write the agent contract into the places coding agents read.
//!
//! Claude and Grok load a skill file from their own directory, so those are
//! written whole. Codex reads a single `AGENTS.md` that usually already has
//! the user's own content, so gist keeps a delimited block inside it and
//! rewrites only that block.

use std::path::{Path, PathBuf};

pub const BEGIN: &str = "<!-- gist:begin -->";
pub const END: &str = "<!-- gist:end -->";

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

    pub fn name(self) -> &'static str {
        match self {
            Target::Claude => "claude",
            Target::Codex => "codex",
            Target::Grok => "grok",
        }
    }

    /// Where this agent reads instructions from, relative to a project root or
    /// to the home directory.
    pub fn path(self, root: &Path, global: bool) -> PathBuf {
        match (self, global) {
            (Target::Claude, false) => root.join(".claude/skills/gist/SKILL.md"),
            (Target::Claude, true) => root.join(".claude/skills/gist/SKILL.md"),
            (Target::Grok, false) => root.join(".grok/skills/gist/SKILL.md"),
            (Target::Grok, true) => root.join(".grok/skills/gist/SKILL.md"),
            (Target::Codex, false) => root.join("AGENTS.md"),
            (Target::Codex, true) => root.join(".codex/AGENTS.md"),
        }
    }

    /// A skill file is owned by gist. `AGENTS.md` is shared, so it gets a block.
    pub fn is_skill(self) -> bool {
        !matches!(self, Target::Codex)
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

/// The same guidance without skill frontmatter, for files like `AGENTS.md`
/// that are prose rather than a skill manifest.
pub fn instructions() -> String {
    BODY.trim_end().to_string()
}

/// Replace an existing gist block, or append one. Anything else in the file is
/// left byte-for-byte alone.
pub fn merge_block(existing: &str, body: &str) -> String {
    let block = format!("{BEGIN}\n{}\n{END}\n", body.trim_end());
    if let (Some(start), Some(end)) = (existing.find(BEGIN), existing.find(END)) {
        if start < end {
            let mut out = String::with_capacity(existing.len() + block.len());
            out.push_str(&existing[..start]);
            out.push_str(&block);
            out.push_str(existing[end + END.len()..].trim_start_matches('\n'));
            return out;
        }
    }
    if existing.trim().is_empty() {
        return block;
    }
    format!("{}\n\n{block}", existing.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_file_has_frontmatter_and_body() {
        let s = skill_file();
        assert!(s.starts_with("---\nname: gist\n"));
        assert!(s.contains("gist put"));
    }

    #[test]
    fn agents_md_text_carries_no_skill_frontmatter() {
        let t = instructions();
        assert!(
            !t.starts_with("---"),
            "frontmatter leaked into AGENTS.md text"
        );
        assert!(t.contains("gist put"));
    }

    #[test]
    fn block_is_appended_then_replaced_in_place() {
        let original = "# My notes\n\nKeep this.\n";
        let once = merge_block(original, "FIRST");
        assert!(once.starts_with("# My notes\n\nKeep this."));
        assert!(once.contains(BEGIN) && once.contains("FIRST"));

        let twice = merge_block(&once, "SECOND");
        assert!(twice.contains("SECOND"));
        assert!(!twice.contains("FIRST"), "stale block left behind");
        assert_eq!(twice.matches(BEGIN).count(), 1, "block duplicated");
        assert!(twice.starts_with("# My notes\n\nKeep this."));
    }

    #[test]
    fn empty_file_gets_just_the_block() {
        let out = merge_block("", "X");
        assert_eq!(out, format!("{BEGIN}\nX\n{END}\n"));
    }

    #[test]
    fn codex_shares_agents_md_others_own_their_file() {
        assert!(!Target::Codex.is_skill());
        assert!(Target::Claude.is_skill() && Target::Grok.is_skill());
        let root = Path::new("/p");
        assert!(Target::Claude
            .path(root, false)
            .ends_with(".claude/skills/gist/SKILL.md"));
        assert!(Target::Grok
            .path(root, false)
            .ends_with(".grok/skills/gist/SKILL.md"));
        assert!(Target::Codex.path(root, false).ends_with("AGENTS.md"));
        assert!(Target::Codex.path(root, true).ends_with(".codex/AGENTS.md"));
    }
}
