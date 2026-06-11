//! Output writers. Two formats:
//!
//! - **JSONL** — one `NormalizedMessage` per line. Consumed by Claude Code
//!   via `--context <file>.jsonl`. Each line is independently parseable.
//! - **Markdown** — hierarchical by `project_hint` then `thread_id`.
//!   Files are written under `<dir>/<project_slug>/<thread_slug>.md`.
//!   Each file has YAML-ish frontmatter and one `## role` section per
//!   message. Optimized for pasting into Claude Projects.
//!
//! The orchestrator (`Pipeline::run`) doesn't write files — it returns
//! a `PipelineOutput` so callers (CLI in F4, MCP server, library users)
//! can decide what to do with the data. These free functions are the
//! convenience layer for the "write to disk" path.

use crate::config::OutputFormat;
use crate::pipeline::{NormalizedMessage, PipelineOutput};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Write a JSONL file. One line per message. Returns the path written.
pub fn write_jsonl(path: &Path, messages: &[NormalizedMessage]) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir for {}", path.display()))?;
        }
    }
    let file = fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    for msg in messages {
        let line = serde_json::to_string(msg).context("serializing NormalizedMessage")?;
        writeln!(writer, "{}", line).context("writing JSONL line")?;
    }
    writer.flush().context("flushing JSONL writer")?;
    Ok(path.to_path_buf())
}

/// Write Markdown files. One file per `project_hint` per `thread_id`.
///
/// Layout:
/// ```text
/// <dir>/
///   <project_slug or "_untitled"/>/
///     <thread_slug>.md
/// ```
///
/// Returns the list of files written, sorted by path.
pub fn write_markdown(dir: &Path, messages: &[NormalizedMessage]) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    // Group: project_hint -> (thread_id, messages).
    // BTreeMap for deterministic ordering across runs.
    let mut grouped: BTreeMap<String, BTreeMap<String, Vec<&NormalizedMessage>>> = BTreeMap::new();
    for msg in messages {
        let project = msg
            .project_hint
            .clone()
            .unwrap_or_else(|| "_untitled".to_string());
        grouped
            .entry(project)
            .or_default()
            .entry(msg.thread_id.clone())
            .or_default()
            .push(msg);
    }

    let mut written: Vec<PathBuf> = Vec::new();
    for (project, threads) in grouped {
        let project_slug = NormalizedMessage::slugify(&project);
        let project_dir = dir.join(&project_slug);
        fs::create_dir_all(&project_dir)
            .with_context(|| format!("creating {}", project_dir.display()))?;
        for (thread_id, msgs) in threads {
            let thread_title = msgs
                .iter()
                .find_map(|m| m.thread_title.clone())
                .unwrap_or_else(|| thread_id.clone());
            let thread_slug = NormalizedMessage::slugify(&thread_title);
            let filename = format!("{}.md", thread_slug);
            let path = project_dir.join(&filename);
            let body = render_markdown_thread(&thread_id, &thread_title, &msgs);
            fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
            written.push(path);
        }
    }
    written.sort();
    Ok(written)
}

/// Dispatch on `OutputFormat`. `Both` writes JSONL to `<dir>/memory.jsonl`
/// and Markdown to `<dir>/`.
pub fn write_all(
    dir: &Path,
    output: &PipelineOutput,
    format: OutputFormat,
) -> Result<Vec<PathBuf>> {
    match format {
        OutputFormat::Jsonl => {
            let path = dir.join("memory.jsonl");
            write_jsonl(&path, &output.messages).map(|p| vec![p])
        }
        OutputFormat::Markdown => write_markdown(dir, &output.messages),
        OutputFormat::Both => {
            let jsonl_path = dir.join("memory.jsonl");
            let jsonl_written = write_jsonl(&jsonl_path, &output.messages)?;
            let md_written = write_markdown(dir, &output.messages)?;
            let mut all = vec![jsonl_written];
            all.extend(md_written);
            all.sort();
            Ok(all)
        }
    }
}

fn render_markdown_thread(
    thread_id: &str,
    thread_title: &str,
    msgs: &[&NormalizedMessage],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", thread_title));
    out.push_str("<!-- metadata\n");
    out.push_str(&format!("thread_id: {}\n", thread_id));
    out.push_str(&format!(
        "source: {}\n",
        msgs.first().map(|m| m.source.as_str()).unwrap_or("")
    ));
    if let Some(first) = msgs.first() {
        out.push_str(&format!("created: {}\n", first.created_at.to_rfc3339()));
    }
    if let Some(last) = msgs.last() {
        out.push_str(&format!("updated: {}\n", last.created_at.to_rfc3339()));
    }
    out.push_str(&format!("messages: {}\n", msgs.len()));
    if let Some(project) = msgs.iter().find_map(|m| m.project_hint.clone()) {
        out.push_str(&format!("project_hint: {}\n", project));
    }
    out.push_str("-->\n\n");

    for msg in msgs {
        let ts = msg.created_at.to_rfc3339();
        out.push_str(&format!("## {} ({})\n\n", msg.role.as_str(), ts));
        out.push_str(&msg.content);
        out.push_str("\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{NormalizedMessage, Role};
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn make_msg(id: &str, thread: &str, project: Option<&str>, role: Role) -> NormalizedMessage {
        NormalizedMessage {
            id: id.into(),
            source: "test".into(),
            role,
            content: format!("body of {}", id),
            original_length: 0,
            created_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            thread_id: thread.into(),
            thread_title: Some(format!("Title for {}", thread)),
            project_hint: project.map(String::from),
            content_hash: 0,
            hits: 1,
            signal_score: 0.5,
        }
    }

    #[test]
    fn jsonl_writes_one_line_per_message() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("memory.jsonl");
        let msgs = vec![
            make_msg("a", "t1", Some("proj"), Role::User),
            make_msg("b", "t1", Some("proj"), Role::Assistant),
        ];
        write_jsonl(&path, &msgs).unwrap();
        let body = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2);
        // Each line is valid JSON
        for line in &lines {
            let parsed: NormalizedMessage = serde_json::from_str(line).unwrap();
            assert!(!parsed.id.is_empty());
        }
    }

    #[test]
    fn markdown_groups_by_project_and_thread() {
        let dir = tempdir().unwrap();
        let msgs = vec![
            make_msg("a", "thread-1", Some("alpha"), Role::User),
            make_msg("b", "thread-1", Some("alpha"), Role::Assistant),
            make_msg("c", "thread-2", Some("alpha"), Role::User),
            make_msg("d", "thread-3", Some("beta"), Role::User),
        ];
        let written = write_markdown(dir.path(), &msgs).unwrap();
        assert_eq!(written.len(), 3, "3 files: 2 in alpha/, 1 in beta/");
        // Project dirs created
        assert!(dir.path().join("alpha").is_dir());
        assert!(dir.path().join("beta").is_dir());
    }

    #[test]
    fn markdown_uses_untitled_for_missing_project() {
        let dir = tempdir().unwrap();
        let msgs = vec![make_msg("a", "t1", None, Role::User)];
        let written = write_markdown(dir.path(), &msgs).unwrap();
        assert_eq!(written.len(), 1);
        // The "_untitled" sentinel is slugified to "untitled" before
        // becoming a directory name. Asserting the slugified form is
        // the user-visible behavior.
        assert!(
            written[0].starts_with(dir.path().join("untitled")),
            "expected dir/untitled/, got {}",
            written[0].display()
        );
    }

    #[test]
    fn write_all_both_produces_jsonl_and_markdown() {
        let dir = tempdir().unwrap();
        let output = PipelineOutput {
            messages: vec![make_msg("a", "t1", Some("proj"), Role::User)],
            stats: Default::default(),
        };
        let written = write_all(dir.path(), &output, OutputFormat::Both).unwrap();
        assert!(written.iter().any(|p| p.ends_with("memory.jsonl")));
        assert!(written
            .iter()
            .any(|p| p.extension().and_then(|s| s.to_str()) == Some("md")));
    }
}
