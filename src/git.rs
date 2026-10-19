//! Real Git backend and workspace inspection service.

use crate::changes_panel::ChangedFile;
use crate::diff_view::{DiffHunk, DiffLine, DiffLineType};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Commit information parsed from git log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    /// Short commit hash (e.g. "a492530").
    pub hash: String,
    /// Author name (e.g. "Antigravity").
    pub author: String,
    /// Relative time description (e.g. "2 hours ago").
    pub relative_time: String,
    /// Single line commit subject.
    pub message: String,
}

/// Workspace project detection result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    /// Display name of the workspace or package (e.g. "Oxidite").
    pub name: String,
    /// Recognized project kind (e.g. "Rust (Cargo)", "Node.js (pnpm)").
    pub kind: String,
    /// Package version if specified (e.g. "v0.1.0").
    pub version: Option<String>,
    /// Root path of the workspace.
    pub root: PathBuf,
}

/// Detects the project kind, name, and version by inspecting files in `root`.
#[must_use]
pub fn detect_project_type(root: &Path) -> ProjectInfo {
    let fallback_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("workspace")
        .to_string();

    // 1. Rust Cargo project
    let cargo_path = root.join("Cargo.toml");
    if cargo_path.exists()
        && let Ok(content) = std::fs::read_to_string(&cargo_path)
    {
        let mut name = None;
        let mut version = None;
        for line in content.lines() {
            let trimmed = line.trim();
            if name.is_none() && trimmed.starts_with("name =") {
                name = trimmed
                    .split('=')
                    .nth(1)
                    .map(|s| s.trim().trim_matches('"').to_string());
            }
            if version.is_none() && trimmed.starts_with("version =") {
                version = trimmed
                    .split('=')
                    .nth(1)
                    .map(|s| format!("v{}", s.trim().trim_matches('"')));
            }
            if name.is_some() && version.is_some() {
                break;
            }
        }
        return ProjectInfo {
            name: name.unwrap_or(fallback_name),
            kind: "Rust (Cargo)".to_string(),
            version,
            root: root.to_path_buf(),
        };
    }

    // 2. Node.js project
    let pkg_json = root.join("package.json");
    if pkg_json.exists()
        && let Ok(content) = std::fs::read_to_string(&pkg_json)
    {
        let mut name = None;
        let mut version = None;
        for line in content.lines() {
            let trimmed = line.trim();
            if name.is_none() && trimmed.starts_with("\"name\":") {
                name = trimmed.split(':').nth(1).map(|s| {
                    s.trim()
                        .trim_matches(|c| c == '"' || c == ',' || c == ' ')
                        .to_string()
                });
            }
            if version.is_none() && trimmed.starts_with("\"version\":") {
                version = trimmed.split(':').nth(1).map(|s| {
                    format!(
                        "v{}",
                        s.trim().trim_matches(|c| c == '"' || c == ',' || c == ' ')
                    )
                });
            }
        }
        return ProjectInfo {
            name: name.unwrap_or(fallback_name),
            kind: "Node.js".to_string(),
            version,
            root: root.to_path_buf(),
        };
    }

    // 3. Python project
    if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        return ProjectInfo {
            name: fallback_name,
            kind: "Python".to_string(),
            version: None,
            root: root.to_path_buf(),
        };
    }

    // 4. Go project
    if root.join("go.mod").exists() {
        return ProjectInfo {
            name: fallback_name,
            kind: "Go".to_string(),
            version: None,
            root: root.to_path_buf(),
        };
    }

    // Fallback workspace
    ProjectInfo {
        name: fallback_name,
        kind: "Workspace".to_string(),
        version: None,
        root: root.to_path_buf(),
    }
}

/// Detects the active git branch name in `cwd`.
#[must_use]
pub fn detect_git_branch(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            return Some(branch);
        }
    }

    // Detached HEAD fallback
    let head_output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if head_output.status.success() {
        let rev = String::from_utf8_lossy(&head_output.stdout)
            .trim()
            .to_string();
        if !rev.is_empty() {
            return Some(format!("({rev})"));
        }
    }

    None
}

/// Queries real git status in `cwd` and returns a list of `ChangedFile`.
/// If the working tree is clean, returns the files changed in the most recent commit
/// so that the review UI displays real repository files instead of empty mocks.
#[must_use]
pub fn query_git_status(cwd: &Path) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let numstat_map = query_numstat(cwd);

    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "-uall"])
        .current_dir(cwd)
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.len() < 4 {
                continue;
            }
            let index_status = line.as_bytes()[0];
            let worktree_status = line.as_bytes()[1];
            let raw_path = line[3..].trim();

            let target_path = if let Some((_, new_path)) = raw_path.split_once(" -> ") {
                new_path.trim()
            } else {
                raw_path
            };

            let path_buf = PathBuf::from(target_path);
            let filename = path_buf
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(target_path)
                .to_string();
            let parent = path_buf
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("")
                .replace('\\', "/");
            let path_info = if parent.is_empty() {
                ".".to_string()
            } else {
                parent
            };

            let is_staged = index_status != b' ' && index_status != b'?';
            let (mut additions, deletions) =
                numstat_map.get(target_path).copied().unwrap_or((0, 0));

            // If untracked file, count lines on disk
            if index_status == b'?' && worktree_status == b'?' && additions == 0 {
                let full_file = cwd.join(target_path);
                if let Ok(content) = std::fs::read_to_string(&full_file) {
                    additions = content.lines().count();
                }
            }

            files.push(ChangedFile {
                filename,
                path_info,
                additions,
                deletions,
                staged: is_staged,
            });
        }
    }

    // If working tree is clean, display the files touched in the latest commit
    if files.is_empty() {
        files = query_latest_commit_files(cwd);
    }

    files
}

/// Helper to query diff numstats for additions and deletions.
fn query_numstat(cwd: &Path) -> HashMap<String, (usize, usize)> {
    let mut map = HashMap::new();

    // Unstaged diff
    if let Ok(out) = Command::new("git")
        .args(["diff", "--numstat"])
        .current_dir(cwd)
        .output()
        && out.status.success()
    {
        parse_numstat_into(&String::from_utf8_lossy(&out.stdout), &mut map);
    }

    // Staged diff
    if let Ok(out) = Command::new("git")
        .args(["diff", "--cached", "--numstat"])
        .current_dir(cwd)
        .output()
        && out.status.success()
    {
        parse_numstat_into(&String::from_utf8_lossy(&out.stdout), &mut map);
    }

    map
}

fn parse_numstat_into(output: &str, map: &mut HashMap<String, (usize, usize)>) {
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let adds = parts[0].parse::<usize>().unwrap_or(0);
            let dels = parts[1].parse::<usize>().unwrap_or(0);
            let path = parts[2].to_string();
            let entry = map.entry(path).or_insert((0, 0));
            entry.0 += adds;
            entry.1 += dels;
        }
    }
}

/// Queries files touched in the latest commit when working tree is clean.
fn query_latest_commit_files(cwd: &Path) -> Vec<ChangedFile> {
    let mut files = Vec::new();

    let output = Command::new("git")
        .args(["show", "--numstat", "--pretty=format:", "HEAD"])
        .current_dir(cwd)
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let additions = parts[0].parse::<usize>().unwrap_or(0);
                let deletions = parts[1].parse::<usize>().unwrap_or(0);
                let full_path = parts[2];
                let path_buf = PathBuf::from(full_path);
                let filename = path_buf
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(full_path)
                    .to_string();
                let parent = path_buf
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .replace('\\', "/");
                let path_info = if parent.is_empty() {
                    ".".to_string()
                } else {
                    parent
                };

                files.push(ChangedFile {
                    filename,
                    path_info,
                    additions,
                    deletions,
                    staged: true, // committed
                });
            }
        }
    }

    files
}

/// Queries commit history log up to `limit` entries.
#[must_use]
pub fn query_git_history(cwd: &Path, limit: usize) -> Vec<GitCommit> {
    let mut commits = Vec::new();
    let limit_arg = format!("-n{limit}");

    let output = Command::new("git")
        .args(["log", &limit_arg, "--pretty=format:%h%x1f%an%x1f%ar%x1f%s"])
        .current_dir(cwd)
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\x1f').collect();
            if parts.len() >= 4 {
                commits.push(GitCommit {
                    hash: parts[0].to_string(),
                    author: parts[1].to_string(),
                    relative_time: parts[2].to_string(),
                    message: parts[3].to_string(),
                });
            }
        }
    }

    commits
}

/// Queries real side-by-side diff hunks for `relative_file_path`.
/// Returns `(file_name, path_info, hunks)`.
#[must_use]
pub fn query_file_diff(cwd: &Path, relative_file_path: &str) -> (String, String, Vec<DiffHunk>) {
    let path_buf = PathBuf::from(relative_file_path);
    let file_name = path_buf
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(relative_file_path)
        .to_string();
    let parent = path_buf
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .replace('\\', "/");
    let path_info = if parent.is_empty() {
        ".".to_string()
    } else {
        format!("{parent}/")
    };

    // 1. Try working tree diff against HEAD
    let mut diff_raw = String::new();
    if let Ok(out) = Command::new("git")
        .args(["diff", "HEAD", "--", relative_file_path])
        .current_dir(cwd)
        .output()
        && out.status.success()
    {
        diff_raw = String::from_utf8_lossy(&out.stdout).to_string();
    }

    // 2. If working tree diff is empty, try staged diff
    if diff_raw.is_empty()
        && let Ok(out) = Command::new("git")
            .args(["diff", "--cached", "--", relative_file_path])
            .current_dir(cwd)
            .output()
        && out.status.success()
    {
        diff_raw = String::from_utf8_lossy(&out.stdout).to_string();
    }

    // 3. If still empty, check the commit diff (HEAD~1..HEAD)
    if diff_raw.is_empty()
        && let Ok(out) = Command::new("git")
            .args(["diff", "HEAD~1..HEAD", "--", relative_file_path])
            .current_dir(cwd)
            .output()
        && out.status.success()
    {
        diff_raw = String::from_utf8_lossy(&out.stdout).to_string();
    }

    if !diff_raw.is_empty() {
        let hunks = parse_unified_diff(&diff_raw);
        if !hunks.is_empty() {
            return (file_name, path_info, hunks);
        }
    }

    // 4. Fallback: load full file as context lines
    let hunks = load_full_file_as_context(cwd, relative_file_path);
    (file_name, path_info, hunks)
}

/// Queries real side-by-side diff hunks introduced in commit `commit_hash`.
/// Returns `(file_name, path_info, hunks)`.
#[must_use]
pub fn query_commit_diff(cwd: &Path, commit_hash: &str) -> (String, String, Vec<DiffHunk>) {
    let output = Command::new("git")
        .args(["show", "-m", "--format=", commit_hash])
        .current_dir(cwd)
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let diff_raw = String::from_utf8_lossy(&out.stdout);
        let hunks = parse_unified_diff(&diff_raw);
        if !hunks.is_empty() {
            return (
                format!("commit {commit_hash}"),
                "git history".to_string(),
                hunks,
            );
        }
    }

    (
        format!("commit {commit_hash}"),
        "git history".to_string(),
        Vec::new(),
    )
}

/// Parses a standard unified diff byte stream into side-by-side `DiffHunk` structures.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn parse_unified_diff(raw_diff: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_header = String::new();
    let mut left_curr: usize = 1;
    let mut right_curr: usize = 1;

    let mut left_lines = Vec::new();
    let mut right_lines = Vec::new();
    let mut pending_dels: Vec<String> = Vec::new();
    let mut pending_adds: Vec<String> = Vec::new();

    let flush_pending = |dels: &mut Vec<String>,
                         adds: &mut Vec<String>,
                         left_lines: &mut Vec<DiffLine>,
                         right_lines: &mut Vec<DiffLine>,
                         left_curr: &mut usize,
                         right_curr: &mut usize| {
        let max_len = dels.len().max(adds.len());
        for i in 0..max_len {
            if i < dels.len() {
                left_lines.push(DiffLine {
                    line_number: Some(*left_curr),
                    content: dels[i].clone(),
                    line_type: DiffLineType::Deletion,
                });
                *left_curr += 1;
            } else {
                left_lines.push(DiffLine {
                    line_number: None,
                    content: String::new(),
                    line_type: DiffLineType::Empty,
                });
            }

            if i < adds.len() {
                right_lines.push(DiffLine {
                    line_number: Some(*right_curr),
                    content: adds[i].clone(),
                    line_type: DiffLineType::Addition,
                });
                *right_curr += 1;
            } else {
                right_lines.push(DiffLine {
                    line_number: None,
                    content: String::new(),
                    line_type: DiffLineType::Empty,
                });
            }
        }
        dels.clear();
        adds.clear();
    };

    for line in raw_diff.lines() {
        if line.starts_with("@@") {
            // Save previous hunk if non-empty
            flush_pending(
                &mut pending_dels,
                &mut pending_adds,
                &mut left_lines,
                &mut right_lines,
                &mut left_curr,
                &mut right_curr,
            );
            if !current_header.is_empty() || !left_lines.is_empty() {
                hunks.push(DiffHunk {
                    header: current_header.clone(),
                    left_lines: std::mem::take(&mut left_lines),
                    right_lines: std::mem::take(&mut right_lines),
                });
            }

            current_header = line.to_string();
            let (l_start, r_start) = parse_hunk_header(line);
            left_curr = l_start;
            right_curr = r_start;
            continue;
        }

        // Inside a hunk
        if current_header.is_empty() {
            continue;
        }

        if let Some(stripped) = line.strip_prefix('-') {
            pending_dels.push(stripped.to_string());
        } else if let Some(stripped) = line.strip_prefix('+') {
            pending_adds.push(stripped.to_string());
        } else if let Some(stripped) = line.strip_prefix(' ') {
            flush_pending(
                &mut pending_dels,
                &mut pending_adds,
                &mut left_lines,
                &mut right_lines,
                &mut left_curr,
                &mut right_curr,
            );
            left_lines.push(DiffLine {
                line_number: Some(left_curr),
                content: stripped.to_string(),
                line_type: DiffLineType::Context,
            });
            right_lines.push(DiffLine {
                line_number: Some(right_curr),
                content: stripped.to_string(),
                line_type: DiffLineType::Context,
            });
            left_curr += 1;
            right_curr += 1;
        }
    }

    flush_pending(
        &mut pending_dels,
        &mut pending_adds,
        &mut left_lines,
        &mut right_lines,
        &mut left_curr,
        &mut right_curr,
    );

    if !current_header.is_empty() || !left_lines.is_empty() {
        hunks.push(DiffHunk {
            header: current_header,
            left_lines,
            right_lines,
        });
    }

    hunks
}

/// Parses the starting line numbers from a unified diff header e.g. "@@ -15,6 +15,7 @@".
fn parse_hunk_header(header: &str) -> (usize, usize) {
    let mut left = 1;
    let mut right = 1;

    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.len() >= 3 {
        if let Some(l_str) = parts[1].strip_prefix('-') {
            let num = l_str.split(',').next().unwrap_or("1");
            left = num.parse::<usize>().unwrap_or(1);
        }
        if let Some(r_str) = parts[2].strip_prefix('+') {
            let num = r_str.split(',').next().unwrap_or("1");
            right = num.parse::<usize>().unwrap_or(1);
        }
    }

    (left, right)
}

/// Loads full file contents into a single context hunk for side-by-side inspection.
fn load_full_file_as_context(cwd: &Path, file_path: &str) -> Vec<DiffHunk> {
    let full_path = cwd.join(file_path);
    let content = std::fs::read_to_string(&full_path).unwrap_or_default();
    let mut left_lines = Vec::new();
    let mut right_lines = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        left_lines.push(DiffLine {
            line_number: Some(line_num),
            content: line.to_string(),
            line_type: DiffLineType::Context,
        });
        right_lines.push(DiffLine {
            line_number: Some(line_num),
            content: line.to_string(),
            line_type: DiffLineType::Context,
        });
    }

    vec![DiffHunk {
        header: format!("@@ 1,{} @@", content.lines().count()),
        left_lines,
        right_lines,
    }]
}

/// Stages a file with `git add <file>`.
pub fn stage_file(cwd: &Path, file_path: &str) -> std::io::Result<()> {
    let status = Command::new("git")
        .args(["add", file_path])
        .current_dir(cwd)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("git add failed"))
    }
}

/// Unstages a file with `git restore --staged <file>`.
pub fn unstage_file(cwd: &Path, file_path: &str) -> std::io::Result<()> {
    let status = Command::new("git")
        .args(["restore", "--staged", file_path])
        .current_dir(cwd)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("git restore --staged failed"))
    }
}

/// Stages all files with `git add -A`.
pub fn stage_all(cwd: &Path) -> std::io::Result<()> {
    let status = Command::new("git")
        .args(["add", "-A"])
        .current_dir(cwd)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("git add -A failed"))
    }
}

/// Commits staged files with `git commit -m <message>`.
pub fn commit_changes(cwd: &Path, message: &str) -> std::io::Result<()> {
    let status = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(cwd)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("git commit failed"))
    }
}

/// Discards uncommitted working-tree modifications for a file with `git restore <file>`.
pub fn restore_file(cwd: &Path, file_path: &str) -> std::io::Result<()> {
    let status = Command::new("git")
        .args(["restore", file_path])
        .current_dir(cwd)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("git restore failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unified_diff_builds_matching_side_by_side_lines() {
        let diff = r"@@ -1,3 +1,4 @@
-old line 1
+new line 1
 context line 2
+extra added line
 context line 3
";
        let hunks = parse_unified_diff(diff);
        assert_eq!(hunks.len(), 1);
        let hunk = &hunks[0];
        assert_eq!(hunk.left_lines.len(), hunk.right_lines.len());

        // Line 1: deletion on left, addition on right
        assert_eq!(hunk.left_lines[0].line_type, DiffLineType::Deletion);
        assert_eq!(hunk.left_lines[0].content, "old line 1");
        assert_eq!(hunk.right_lines[0].line_type, DiffLineType::Addition);
        assert_eq!(hunk.right_lines[0].content, "new line 1");

        // Line 2: Context
        assert_eq!(hunk.left_lines[1].line_type, DiffLineType::Context);
        assert_eq!(hunk.right_lines[1].line_type, DiffLineType::Context);
        assert_eq!(hunk.left_lines[1].content, "context line 2");

        // Line 3: Empty on left, Addition on right
        assert_eq!(hunk.left_lines[2].line_type, DiffLineType::Empty);
        assert_eq!(hunk.right_lines[2].line_type, DiffLineType::Addition);
        assert_eq!(hunk.right_lines[2].content, "extra added line");

        // Line 4: Context
        assert_eq!(hunk.left_lines[3].line_type, DiffLineType::Context);
        assert_eq!(hunk.right_lines[3].line_type, DiffLineType::Context);
        assert_eq!(hunk.left_lines[3].content, "context line 3");
    }

    #[test]
    fn detect_project_type_identifies_cargo_workspace() {
        let cwd = std::env::current_dir().expect("cwd exists");
        let info = detect_project_type(&cwd);
        assert_eq!(info.kind, "Rust (Cargo)");
        assert_eq!(info.name, "oxidite");
        assert!(info.version.is_some());
    }

    #[test]
    fn parse_hunk_header_extracts_correct_start_lines() {
        let (left, right) = parse_hunk_header("@@ -42,10 +88,15 @@ fn test()");
        assert_eq!(left, 42);
        assert_eq!(right, 88);
    }
}
