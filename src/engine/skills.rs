//! Skill system for the new engine.
//!
//! Skills are user-defined prompts stored as directories containing a
//! `SKILL.md` file with YAML frontmatter. The registry scans multiple
//! directories (project-level then global) and indexes them for command
//! dispatch.
//!
//! ## Integration with engine commands
//!
//! After checking built-in and custom commands in `commands::dispatch`,
//! the engine should check skills:
//!
//! ```rust,ignore
//! if let Some(skill) = skill_registry.resolve(cmd) {
//!     let prompt = skills::build_skill_invocation_prompt(skill, args);
//!     // Send `prompt` to the agent instead of replying directly.
//! }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Skill definition
// ---------------------------------------------------------------------------

/// A loaded skill definition parsed from a `SKILL.md` file.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Canonical name from the YAML frontmatter (e.g. `super-aidlc`).
    pub name: String,
    /// Human-readable display name (derived from `name` if not set).
    pub display_name: String,
    /// Short description from the YAML frontmatter.
    pub description: String,
    /// The prompt body (everything after the second `---` marker).
    pub prompt: String,
    /// Path to the `SKILL.md` file this was loaded from.
    pub source: PathBuf,
}

// ---------------------------------------------------------------------------
// Skill registry
// ---------------------------------------------------------------------------

/// Registry that scans and indexes skills from multiple directories.
///
/// Directories are scanned in priority order -- the first match wins.
/// This allows project-level skills to override global ones.
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    /// Create a new registry by scanning directories in priority order.
    ///
    /// `scan_dirs` should be ordered from highest to lowest priority
    /// (e.g. project-level first, then global). First match wins:
    /// if both directories contain a skill with the same canonical name,
    /// the one from the earlier directory is kept.
    pub fn new(scan_dirs: &[PathBuf]) -> Self {
        let mut skills = HashMap::new();
        for dir in scan_dirs {
            Self::scan_dir(dir, &mut skills);
        }
        SkillRegistry { skills }
    }

    /// Scan a single directory for skills.
    ///
    /// Each subdirectory that contains a `SKILL.md` file is treated as a
    /// skill. Skills already present in the map (from a higher-priority
    /// directory) are not overwritten.
    fn scan_dir(dir: &Path, skills: &mut HashMap<String, Skill>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return, // Directory doesn't exist or not readable.
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Resolve symlinks so we can check the actual target.
            let resolved = match std::fs::metadata(&path) {
                Ok(m) if m.is_dir() => path.clone(),
                // The entry itself might be a symlink to a directory.
                _ => match std::fs::read_link(&path) {
                    Ok(target) => {
                        let abs_target = if target.is_absolute() {
                            target
                        } else {
                            dir.join(&target)
                        };
                        if abs_target.is_dir() {
                            abs_target
                        } else {
                            continue;
                        }
                    }
                    Err(_) => continue,
                },
            };

            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let skill_file = resolved.join("SKILL.md");
            if !skill_file.exists() {
                // Also try following the symlink for SKILL.md itself.
                if let Ok(meta) = std::fs::metadata(&skill_file) {
                    if !meta.is_file() {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            if let Some(skill) = Self::parse_skill_file(&skill_file, &dir_name) {
                let canonical = skill.name.to_lowercase();
                // First match wins -- do not overwrite higher-priority entries.
                skills.entry(canonical).or_insert(skill);
            }
        }
    }

    /// Parse a `SKILL.md` file into a [`Skill`].
    ///
    /// The file format is YAML frontmatter between `---` markers followed
    /// by the prompt body:
    ///
    /// ```text
    /// ---
    /// name: my-skill
    /// description: Does something useful
    /// ---
    ///
    /// # My Skill
    /// The user wants to: $ARGUMENTS
    /// ...
    /// ```
    fn parse_skill_file(path: &Path, dir_name: &str) -> Option<Skill> {
        let content = std::fs::read_to_string(path).ok()?;
        let (name, description, prompt) = parse_frontmatter(&content, dir_name)?;

        let display_name = name
            .split(':')
            .next_back()
            .unwrap_or(&name)
            .split('-')
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => {
                        let mut s = c.to_uppercase().to_string();
                        s.push_str(&chars.as_str().to_lowercase());
                        s
                    }
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        Some(Skill {
            name,
            display_name,
            description,
            prompt,
            source: path.to_path_buf(),
        })
    }

    /// Resolve a command name to a skill.
    ///
    /// Tries, in order:
    /// 1. Exact match (case-insensitive)
    /// 2. Sanitized match (replace non-alphanumeric with `_`, lowercase)
    pub fn resolve(&self, name: &str) -> Option<&Skill> {
        let lower = name.to_lowercase();

        // 1. Exact match (case-insensitive).
        if let Some(skill) = self.skills.get(&lower) {
            return Some(skill);
        }

        // 2. Sanitized match: the user may type `super_aidlc` for `super-aidlc`.
        let sanitized_input = sanitize_name(name);
        for (canonical, skill) in &self.skills {
            if sanitize_name(canonical) == sanitized_input {
                return Some(skill);
            }
        }

        None
    }

    /// List all registered skills, sorted by name.
    pub fn list_all(&self) -> Vec<&Skill> {
        let mut skills: Vec<_> = self.skills.values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

/// Parse YAML frontmatter from a SKILL.md file.
///
/// Returns `(name, description, prompt_body)`.
/// Falls back to `dir_name` if the frontmatter does not contain a `name` field.
fn parse_frontmatter(content: &str, dir_name: &str) -> Option<(String, String, String)> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        // No frontmatter -- entire content is the prompt, use dir name.
        return Some((dir_name.to_string(), String::new(), content.to_string()));
    }

    // Find the closing `---`.
    let after_first = &trimmed[3..];
    let closing_pos = after_first.find("\n---")?;

    let yaml_block = &after_first[..closing_pos].trim();
    let prompt_start = 3 + closing_pos + 4; // skip "\n---"
    let prompt = if prompt_start < trimmed.len() {
        trimmed[prompt_start..].trim_start_matches('\n').to_string()
    } else {
        String::new()
    };

    // Parse the YAML block.
    let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_block).ok()?;
    let mapping = yaml.as_mapping()?;

    let name = mapping
        .get(serde_yaml::Value::String("name".to_string()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| dir_name.to_string());

    let description = mapping
        .get(serde_yaml::Value::String("description".to_string()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    Some((name, description, prompt))
}

// ---------------------------------------------------------------------------
// Name sanitization
// ---------------------------------------------------------------------------

/// Sanitize a skill name for Telegram commands (only `[a-z0-9_]` allowed).
///
/// - Lowercases the input
/// - Replaces colons and hyphens (and any other non-alphanumeric char) with `_`
/// - Collapses consecutive underscores
/// - Strips leading/trailing underscores
pub fn sanitize_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut prev_underscore = false;

    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore {
            result.push('_');
            prev_underscore = true;
        }
    }

    // Trim leading/trailing underscores.
    result.trim_matches('_').to_string()
}

// ---------------------------------------------------------------------------
// Skill invocation prompt
// ---------------------------------------------------------------------------

/// Build the invocation prompt sent to the agent when a user triggers a skill.
pub fn build_skill_invocation_prompt(skill: &Skill, args: &str) -> String {
    let mut prompt = String::new();
    prompt.push_str("The user is asking you to execute the following skill.\n\n");
    prompt.push_str(&format!("## Skill: {}\n", skill.display_name));
    if !skill.description.is_empty() {
        prompt.push_str(&format!("## Description: {}\n", skill.description));
    }
    prompt.push_str("\n## Skill Instructions:\n");
    prompt.push_str(&skill.prompt);
    if !args.is_empty() {
        prompt.push_str("\n\n## User Arguments:\n");
        prompt.push_str(args);
    }
    prompt.push_str("\n\nPlease follow the skill instructions above to complete the task.");
    prompt
}

/// Build the default scan directories for a given work directory.
///
/// Returns directories in priority order:
/// 1. `{work_dir}/.claude/skills/` -- project-level
/// 2. `~/.claude/skills/` -- user global
pub fn default_scan_dirs(work_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![work_dir.join(".claude").join("skills")];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".claude").join("skills"));
    }
    dirs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // -- parse_frontmatter ---------------------------------------------------

    #[test]
    fn parse_frontmatter_full() {
        let content = "\
---
name: my-skill
description: Does cool things
---

# My Skill

Do the thing.
";
        let (name, desc, prompt) = parse_frontmatter(content, "fallback").unwrap();
        assert_eq!(name, "my-skill");
        assert_eq!(desc, "Does cool things");
        assert!(prompt.starts_with("# My Skill"));
        assert!(prompt.contains("Do the thing."));
    }

    #[test]
    fn parse_frontmatter_no_name_uses_dir() {
        let content = "\
---
description: Only a description
---

Prompt body here.
";
        let (name, desc, prompt) = parse_frontmatter(content, "dir-name").unwrap();
        assert_eq!(name, "dir-name");
        assert_eq!(desc, "Only a description");
        assert!(prompt.contains("Prompt body here."));
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let content = "# Just a prompt\n\nNo frontmatter here.";
        let (name, desc, prompt) = parse_frontmatter(content, "fallback-dir").unwrap();
        assert_eq!(name, "fallback-dir");
        assert_eq!(desc, "");
        assert!(prompt.contains("Just a prompt"));
    }

    #[test]
    fn parse_frontmatter_empty_prompt() {
        let content = "\
---
name: empty-body
description: Nothing after
---
";
        let (name, desc, prompt) = parse_frontmatter(content, "x").unwrap();
        assert_eq!(name, "empty-body");
        assert_eq!(desc, "Nothing after");
        assert!(prompt.is_empty());
    }

    #[test]
    fn parse_frontmatter_extra_fields_ignored() {
        let content = "\
---
name: with-extras
description: Has extra fields
argument-hint: \"[stuff]\"
model: opus
---

The prompt.
";
        let (name, desc, prompt) = parse_frontmatter(content, "x").unwrap();
        assert_eq!(name, "with-extras");
        assert_eq!(desc, "Has extra fields");
        assert!(prompt.contains("The prompt."));
    }

    // -- sanitize_name -------------------------------------------------------

    #[test]
    fn sanitize_simple_hyphen() {
        assert_eq!(sanitize_name("review-pr"), "review_pr");
    }

    #[test]
    fn sanitize_uppercase_spaces() {
        assert_eq!(sanitize_name("Super AIDLC"), "super_aidlc");
    }

    #[test]
    fn sanitize_colon_separator() {
        assert_eq!(sanitize_name("super-aidlc:brainstorm"), "super_aidlc_brainstorm");
    }

    #[test]
    fn sanitize_already_clean() {
        assert_eq!(sanitize_name("simple"), "simple");
    }

    #[test]
    fn sanitize_consecutive_specials() {
        assert_eq!(sanitize_name("a--b__c"), "a_b_c");
    }

    #[test]
    fn sanitize_leading_trailing() {
        assert_eq!(sanitize_name("-leading-trailing-"), "leading_trailing");
    }

    // -- SkillRegistry::scan_dir & resolve -----------------------------------

    fn create_test_skill_dir(base: &Path, dir_name: &str, content: &str) {
        let skill_dir = base.join(dir_name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn scan_directory_with_multiple_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        create_test_skill_dir(
            base,
            "alpha",
            "---\nname: alpha\ndescription: First skill\n---\n\nAlpha prompt.",
        );
        create_test_skill_dir(
            base,
            "beta",
            "---\nname: beta\ndescription: Second skill\n---\n\nBeta prompt.",
        );

        let registry = SkillRegistry::new(&[base.to_path_buf()]);
        let all = registry.list_all();
        assert_eq!(all.len(), 2);

        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[test]
    fn resolve_exact_match() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill_dir(
            tmp.path(),
            "my-skill",
            "---\nname: my-skill\ndescription: Test\n---\n\nPrompt.",
        );

        let registry = SkillRegistry::new(&[tmp.path().to_path_buf()]);
        assert!(registry.resolve("my-skill").is_some());
        assert_eq!(registry.resolve("my-skill").unwrap().name, "my-skill");
    }

    #[test]
    fn resolve_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill_dir(
            tmp.path(),
            "my-skill",
            "---\nname: My-Skill\ndescription: Test\n---\n\nPrompt.",
        );

        let registry = SkillRegistry::new(&[tmp.path().to_path_buf()]);
        assert!(registry.resolve("MY-SKILL").is_some());
        assert!(registry.resolve("my-skill").is_some());
    }

    #[test]
    fn resolve_sanitized_match() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill_dir(
            tmp.path(),
            "super-aidlc",
            "---\nname: super-aidlc\ndescription: Test\n---\n\nPrompt.",
        );

        let registry = SkillRegistry::new(&[tmp.path().to_path_buf()]);
        // User types underscore instead of hyphen.
        assert!(registry.resolve("super_aidlc").is_some());
        assert_eq!(
            registry.resolve("super_aidlc").unwrap().name,
            "super-aidlc"
        );
    }

    #[test]
    fn resolve_no_match() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill_dir(
            tmp.path(),
            "alpha",
            "---\nname: alpha\ndescription: A\n---\n\nPrompt.",
        );

        let registry = SkillRegistry::new(&[tmp.path().to_path_buf()]);
        assert!(registry.resolve("nonexistent").is_none());
    }

    #[test]
    fn project_level_overrides_global() {
        let project = tempfile::tempdir().unwrap();
        let global = tempfile::tempdir().unwrap();

        create_test_skill_dir(
            project.path(),
            "shared",
            "---\nname: shared\ndescription: Project version\n---\n\nProject prompt.",
        );
        create_test_skill_dir(
            global.path(),
            "shared",
            "---\nname: shared\ndescription: Global version\n---\n\nGlobal prompt.",
        );

        let registry = SkillRegistry::new(&[
            project.path().to_path_buf(),
            global.path().to_path_buf(),
        ]);

        let skill = registry.resolve("shared").unwrap();
        assert_eq!(skill.description, "Project version");
        assert!(skill.prompt.contains("Project prompt."));
    }

    #[test]
    fn scan_nonexistent_directory_is_ok() {
        let registry = SkillRegistry::new(&[PathBuf::from("/nonexistent/path/skills")]);
        assert!(registry.list_all().is_empty());
    }

    // -- build_skill_invocation_prompt ----------------------------------------

    #[test]
    fn invocation_prompt_with_args() {
        let skill = Skill {
            name: "test-skill".to_string(),
            display_name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            prompt: "Do the thing with: $ARGUMENTS".to_string(),
            source: PathBuf::from("/tmp/test-skill/SKILL.md"),
        };

        let prompt = build_skill_invocation_prompt(&skill, "build a widget");

        assert!(prompt.contains("## Skill: Test Skill"));
        assert!(prompt.contains("## Description: A test skill"));
        assert!(prompt.contains("## Skill Instructions:"));
        assert!(prompt.contains("Do the thing with: $ARGUMENTS"));
        assert!(prompt.contains("## User Arguments:"));
        assert!(prompt.contains("build a widget"));
        assert!(prompt.contains("Please follow the skill instructions above"));
    }

    #[test]
    fn invocation_prompt_without_args() {
        let skill = Skill {
            name: "no-args".to_string(),
            display_name: "No Args".to_string(),
            description: "".to_string(),
            prompt: "Just run.".to_string(),
            source: PathBuf::from("/tmp/no-args/SKILL.md"),
        };

        let prompt = build_skill_invocation_prompt(&skill, "");

        assert!(prompt.contains("## Skill: No Args"));
        // No description line when empty.
        assert!(!prompt.contains("## Description:"));
        assert!(prompt.contains("Just run."));
        // No user arguments section when empty.
        assert!(!prompt.contains("## User Arguments:"));
    }

    #[test]
    fn invocation_prompt_no_description() {
        let skill = Skill {
            name: "minimal".to_string(),
            display_name: "Minimal".to_string(),
            description: "".to_string(),
            prompt: "Minimal prompt.".to_string(),
            source: PathBuf::from("/tmp/minimal/SKILL.md"),
        };

        let prompt = build_skill_invocation_prompt(&skill, "some args");
        assert!(!prompt.contains("## Description:"));
        assert!(prompt.contains("## User Arguments:\nsome args"));
    }

    // -- display_name generation ---------------------------------------------

    #[test]
    fn display_name_from_hyphenated() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill_dir(
            tmp.path(),
            "review-pr",
            "---\nname: review-pr\ndescription: Reviews\n---\n\nReview it.",
        );

        let registry = SkillRegistry::new(&[tmp.path().to_path_buf()]);
        let skill = registry.resolve("review-pr").unwrap();
        assert_eq!(skill.display_name, "Review Pr");
    }

    #[test]
    fn display_name_colon_takes_last_segment() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill_dir(
            tmp.path(),
            "super-aidlc-brainstorm",
            "---\nname: super-aidlc:brainstorm\ndescription: Brainstorm\n---\n\nExplore.",
        );

        let registry = SkillRegistry::new(&[tmp.path().to_path_buf()]);
        let skill = registry.resolve("super-aidlc:brainstorm").unwrap();
        assert_eq!(skill.display_name, "Brainstorm");
    }

    // -- default_scan_dirs ----------------------------------------------------

    #[test]
    fn default_scan_dirs_includes_work_dir() {
        let dirs = default_scan_dirs(Path::new("/project"));
        assert_eq!(dirs[0], PathBuf::from("/project/.claude/skills"));
        // Second should be the home directory + .claude/skills.
        assert!(dirs.len() >= 2);
    }
}
