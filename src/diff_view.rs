//! Side-by-side code diff viewer and in-built code review workspace (visual center pane).

use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
use glyphon::Color as GlyphonColor;

/// Height of the top diff header in logical pixels.
pub const DIFF_HEADER_HEIGHT: f32 = 34.0;
/// Line height in logical pixels for each diff code line.
pub const DIFF_LINE_HEIGHT: f32 = 20.0;
/// Height in logical pixels for an inline code review comment card.
pub const DIFF_COMMENT_HEIGHT: f32 = 22.0;
/// Width in logical pixels of the line number gutter on each column.
pub const DIFF_GUTTER_WIDTH: f32 = 42.0;

/// Type of line in a side-by-side code diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineType {
    /// Neutral unchanged line.
    Context,
    /// Added line (green highlight).
    Addition,
    /// Removed line (red highlight).
    Deletion,
    /// Blank line for vertical alignment with opposite column.
    Empty,
}

/// Column side in a side-by-side diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    /// Left base / original column.
    Original,
    /// Right modified / new column.
    Modified,
}

/// Overall review verdict for the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    /// Actively under review.
    InReview,
    /// All changes approved.
    Approved,
    /// Changes requested by reviewer.
    ChangesRequested,
}

impl ReviewVerdict {
    /// Badge label for the status ribbon.
    #[must_use]
    pub fn badge_text(self) -> &'static str {
        match self {
            ReviewVerdict::InReview => "In Review",
            ReviewVerdict::Approved => "Approved",
            ReviewVerdict::ChangesRequested => "Changes",
        }
    }

    /// Color representing this verdict in [r, g, b, a].
    #[must_use]
    pub fn badge_color(self) -> [f32; 4] {
        match self {
            ReviewVerdict::InReview => [0.91, 0.72, 0.36, 0.95], // Amber
            ReviewVerdict::Approved => [0.21, 0.82, 0.50, 0.95], // Emerald green
            ReviewVerdict::ChangesRequested => [1.0, 0.38, 0.42, 0.95], // Danger red
        }
    }

    /// Cycles to the next verdict state.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            ReviewVerdict::InReview => ReviewVerdict::Approved,
            ReviewVerdict::Approved => ReviewVerdict::ChangesRequested,
            ReviewVerdict::ChangesRequested => ReviewVerdict::InReview,
        }
    }
}

/// Tag categorization for inline review comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewTag {
    /// Small cosmetic or style note.
    Nit,
    /// Improvement suggestion.
    Suggestion,
    /// Critical defect or bug.
    Issue,
    /// Commendation or approval.
    Approval,
    /// General observation.
    Note,
}

impl ReviewTag {
    /// Cycles to the next review tag.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            ReviewTag::Nit => ReviewTag::Suggestion,
            ReviewTag::Suggestion => ReviewTag::Issue,
            ReviewTag::Issue => ReviewTag::Approval,
            ReviewTag::Approval => ReviewTag::Note,
            ReviewTag::Note => ReviewTag::Nit,
        }
    }

    /// Short display tag label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ReviewTag::Nit => "NIT",
            ReviewTag::Suggestion => "SUGGEST",
            ReviewTag::Issue => "ISSUE",
            ReviewTag::Approval => "LGTM",
            ReviewTag::Note => "NOTE",
        }
    }

    /// Primary accent color for the tag.
    #[must_use]
    pub fn color(self) -> [f32; 4] {
        match self {
            ReviewTag::Nit => [0.65, 0.68, 0.75, 1.0],
            ReviewTag::Suggestion => [0.47, 0.67, 1.0, 1.0],
            ReviewTag::Issue => [1.0, 0.45, 0.45, 1.0],
            ReviewTag::Approval => [0.35, 0.85, 0.55, 1.0],
            ReviewTag::Note => [0.95, 0.75, 0.35, 1.0],
        }
    }
}

/// An interactive in-progress review comment draft attached to a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentDraft {
    /// File line number being reviewed.
    pub line_number: usize,
    /// Column side (original or modified).
    pub side: DiffSide,
    /// Tag categorization.
    pub tag: ReviewTag,
    /// Editable draft content string.
    pub content: String,
}

impl CommentDraft {
    /// Creates a new `CommentDraft` on `line_number` and `side`.
    #[must_use]
    pub fn new(line_number: usize, side: DiffSide) -> Self {
        Self {
            line_number,
            side,
            tag: ReviewTag::Suggestion,
            content: String::new(),
        }
    }
}

/// A span of syntax highlighted text within a diff code line.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxSpan {
    /// Token text string.
    pub text: String,
    /// Color for this token in glyphon RGBA.
    pub color: GlyphonColor,
    /// Whether the token is bold (e.g. keywords).
    pub bold: bool,
}

/// Tokenizes a single line of source code into syntax spans based on file extension.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn tokenize_syntax_line(
    content: &str,
    file_name: &str,
    line_type: DiffLineType,
) -> Vec<SyntaxSpan> {
    if content.is_empty() {
        return Vec::new();
    }

    let ext = std::path::Path::new(file_name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let is_rust = ext == "rs";
    let is_js = matches!(ext.as_str(), "js" | "jsx" | "ts" | "tsx" | "mjs");
    let is_python = ext == "py";
    let is_c = matches!(ext.as_str(), "c" | "cpp" | "h" | "hpp" | "cc");
    let is_config = matches!(ext.as_str(), "toml" | "json" | "yaml" | "yml");

    let default_color = match line_type {
        DiffLineType::Deletion => GlyphonColor::rgb(255, 175, 180),
        DiffLineType::Addition => GlyphonColor::rgb(165, 235, 185),
        _ => GlyphonColor::rgb(215, 220, 235),
    };

    let keyword_color = GlyphonColor::rgb(195, 140, 245);
    let type_color = GlyphonColor::rgb(235, 195, 115);
    let string_color = GlyphonColor::rgb(140, 220, 165);
    let number_color = GlyphonColor::rgb(235, 165, 105);
    let comment_color = GlyphonColor::rgb(115, 125, 145);

    let mut spans = Vec::new();
    let mut chars = content.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        // 1. Whitespace
        if ch.is_whitespace() {
            let mut ws = String::from(ch);
            while let Some(&(_, next_ch)) = chars.peek() {
                if next_ch.is_whitespace() {
                    ws.push(next_ch);
                    chars.next();
                } else {
                    break;
                }
            }
            spans.push(SyntaxSpan {
                text: ws,
                color: default_color,
                bold: false,
            });
            continue;
        }

        // 2. Comments
        if (ch == '/' && chars.peek().map(|&(_, c)| c) == Some('/'))
            || (ch == '#' && (is_python || is_config || ext == "sh" || ext == "bash"))
        {
            let comment_text = &content[i..];
            spans.push(SyntaxSpan {
                text: comment_text.to_string(),
                color: comment_color,
                bold: false,
            });
            break;
        }

        // 3. String literals
        if ch == '"' || ch == '\'' || (ch == '`' && is_js) {
            let quote = ch;
            let mut s = String::from(quote);
            let mut escaped = false;
            for (_, next_ch) in chars.by_ref() {
                s.push(next_ch);
                if escaped {
                    escaped = false;
                } else if next_ch == '\\' {
                    escaped = true;
                } else if next_ch == quote {
                    break;
                }
            }
            spans.push(SyntaxSpan {
                text: s,
                color: string_color,
                bold: false,
            });
            continue;
        }

        // 4. Numbers
        if ch.is_ascii_digit() {
            let mut num = String::from(ch);
            while let Some(&(_, next_ch)) = chars.peek() {
                if next_ch.is_ascii_alphanumeric() || next_ch == '.' || next_ch == '_' {
                    num.push(next_ch);
                    chars.next();
                } else {
                    break;
                }
            }
            spans.push(SyntaxSpan {
                text: num,
                color: number_color,
                bold: false,
            });
            continue;
        }

        // 5. Identifiers / Keywords / Types
        if ch.is_alphabetic() || ch == '_' {
            let mut word = String::from(ch);
            while let Some(&(_, next_ch)) = chars.peek() {
                if next_ch.is_alphanumeric() || next_ch == '_' {
                    word.push(next_ch);
                    chars.next();
                } else {
                    break;
                }
            }

            let is_kw = if is_rust {
                matches!(
                    word.as_str(),
                    "fn" | "let"
                        | "mut"
                        | "pub"
                        | "struct"
                        | "enum"
                        | "impl"
                        | "trait"
                        | "use"
                        | "mod"
                        | "match"
                        | "if"
                        | "else"
                        | "return"
                        | "while"
                        | "for"
                        | "in"
                        | "loop"
                        | "async"
                        | "await"
                        | "true"
                        | "false"
                        | "self"
                        | "super"
                        | "crate"
                )
            } else if is_js {
                matches!(
                    word.as_str(),
                    "function"
                        | "const"
                        | "let"
                        | "var"
                        | "if"
                        | "else"
                        | "return"
                        | "import"
                        | "export"
                        | "from"
                        | "default"
                        | "class"
                        | "async"
                        | "await"
                        | "true"
                        | "false"
                        | "null"
                        | "undefined"
                )
            } else if is_python {
                matches!(
                    word.as_str(),
                    "def"
                        | "class"
                        | "if"
                        | "elif"
                        | "else"
                        | "return"
                        | "import"
                        | "from"
                        | "as"
                        | "while"
                        | "for"
                        | "in"
                        | "try"
                        | "except"
                        | "with"
                        | "lambda"
                        | "True"
                        | "False"
                        | "None"
                )
            } else if is_c {
                matches!(
                    word.as_str(),
                    "int"
                        | "char"
                        | "void"
                        | "return"
                        | "if"
                        | "else"
                        | "struct"
                        | "class"
                        | "public"
                        | "private"
                        | "include"
                        | "const"
                        | "static"
                        | "auto"
                        | "template"
                        | "namespace"
                )
            } else {
                matches!(
                    word.as_str(),
                    "fn" | "let"
                        | "const"
                        | "def"
                        | "function"
                        | "return"
                        | "if"
                        | "else"
                        | "true"
                        | "false"
                )
            };

            let is_ty = if is_rust {
                matches!(
                    word.as_str(),
                    "String"
                        | "str"
                        | "usize"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "i8"
                        | "i16"
                        | "i32"
                        | "i64"
                        | "f32"
                        | "f64"
                        | "bool"
                        | "char"
                        | "Option"
                        | "Result"
                        | "Some"
                        | "None"
                        | "Ok"
                        | "Err"
                        | "Vec"
                        | "Arc"
                        | "Mutex"
                        | "Box"
                        | "Self"
                )
            } else if is_js {
                matches!(
                    word.as_str(),
                    "string"
                        | "number"
                        | "boolean"
                        | "any"
                        | "void"
                        | "Promise"
                        | "true"
                        | "false"
                        | "null"
                        | "undefined"
                )
            } else if is_python {
                matches!(
                    word.as_str(),
                    "int"
                        | "str"
                        | "float"
                        | "bool"
                        | "list"
                        | "dict"
                        | "set"
                        | "True"
                        | "False"
                        | "None"
                )
            } else {
                word.starts_with(|c: char| c.is_uppercase())
            };

            if is_kw {
                spans.push(SyntaxSpan {
                    text: word,
                    color: keyword_color,
                    bold: true,
                });
            } else if is_ty {
                spans.push(SyntaxSpan {
                    text: word,
                    color: type_color,
                    bold: false,
                });
            } else {
                spans.push(SyntaxSpan {
                    text: word,
                    color: default_color,
                    bold: false,
                });
            }
            continue;
        }

        // 6. Operators and Punctuation
        spans.push(SyntaxSpan {
            text: ch.to_string(),
            color: default_color,
            bold: false,
        });
    }

    spans
}

/// An inline code review comment attached to a specific line in the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewComment {
    /// Unique identifier for this comment.
    pub id: usize,
    /// File line number being reviewed.
    pub line_number: usize,
    /// Column side (original or modified).
    pub side: DiffSide,
    /// Author / reviewer name.
    pub author: String,
    /// Tag categorization.
    pub tag: ReviewTag,
    /// Comment body text.
    pub content: String,
    /// Whether this comment discussion has been resolved.
    pub resolved: bool,
}

/// A single line of text with line number and diff category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Line number in file, if not an empty spacer line.
    pub line_number: Option<usize>,
    /// Text content of this line.
    pub content: String,
    /// Diff status (addition, deletion, context, empty).
    pub line_type: DiffLineType,
}

/// A contiguous diff hunk containing original (left) and modified (right) lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// Hunk header description (e.g. "@@ -1,9 +1,10 @@").
    pub header: String,
    /// Lines on the left (base/original) column.
    pub left_lines: Vec<DiffLine>,
    /// Lines on the right (modified) column.
    pub right_lines: Vec<DiffLine>,
}

/// State for the side-by-side diff review workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffViewState {
    /// Active file name being reviewed (e.g. "sw.js").
    pub file_name: String,
    /// Path / folder metadata (e.g. "ExamCooker-2024/public/").
    pub path_info: String,
    /// Diff hunks for this file.
    pub hunks: Vec<DiffHunk>,
    /// Scroll offset in lines.
    pub scroll_offset: usize,
    /// Whether this file is currently staged.
    pub staged: bool,
    /// Whether review mode toolbar and annotations are active.
    pub review_mode: bool,
    /// Current review verdict (`InReview`, `Approved`, `ChangesRequested`).
    pub review_verdict: ReviewVerdict,
    /// Inline review comments attached to lines in this file.
    pub comments: Vec<ReviewComment>,
    /// Auto-incrementing identifier counter for new comments.
    pub next_comment_id: usize,
    /// Currently focused or selected line (`line_number`, side).
    pub selected_line: Option<(usize, DiffSide)>,
    /// Current file index in the multi-file review list (0-based).
    pub file_index: usize,
    /// Total number of changed files in review list.
    pub total_files: usize,
    /// In-progress inline comment draft if editing.
    pub comment_draft: Option<CommentDraft>,
}

impl DiffViewState {
    /// Adds an inline code review comment and returns its unique ID.
    pub fn add_comment(
        &mut self,
        line_number: usize,
        side: DiffSide,
        tag: ReviewTag,
        content: String,
        author: String,
    ) -> usize {
        let id = self.next_comment_id;
        self.next_comment_id += 1;
        self.comments.push(ReviewComment {
            id,
            line_number,
            side,
            author,
            tag,
            content,
            resolved: false,
        });
        id
    }

    /// Toggles the resolved state of a comment by ID.
    pub fn toggle_resolve(&mut self, comment_id: usize) {
        if let Some(comment) = self.comments.iter_mut().find(|c| c.id == comment_id) {
            comment.resolved = !comment.resolved;
        }
    }

    /// Deletes a comment by ID.
    pub fn delete_comment(&mut self, comment_id: usize) {
        self.comments.retain(|c| c.id != comment_id);
    }

    /// Sets the overall review verdict.
    pub fn set_verdict(&mut self, verdict: ReviewVerdict) {
        self.review_verdict = verdict;
    }

    /// Performs an automated AI code review inspecting hunks for common bugs,
    /// potential panics, unresolved TODOs, debug statements, and security issues.
    pub fn run_ai_review(&mut self) -> usize {
        // Clear previous AI comments to avoid duplicating on re-run
        self.comments.retain(|c| !c.author.contains("AI"));

        let mut new_comments = Vec::new();
        let mut issues_found = 0;

        for hunk in &self.hunks {
            for line in &hunk.right_lines {
                if line.line_type == DiffLineType::Addition && let Some(ln) = line.line_number {
                    let content = line.content.trim();

                    // 1. Check for unhandled unwrap / expect
                    if content.contains(".unwrap()") {
                        new_comments.push((
                            ln,
                            DiffSide::Modified,
                            ReviewTag::Issue,
                            "Avoid .unwrap() in production code; handle error with Result, ? or provide a fallback."
                                .to_string(),
                        ));
                        issues_found += 1;
                    } else if content.contains(".expect(") {
                        new_comments.push((
                            ln,
                            DiffSide::Modified,
                            ReviewTag::Suggestion,
                            "Potential panic via .expect(): verify this invariant is guaranteed never to fail."
                                .to_string(),
                        ));
                    }

                    // 2. Check for leftover TODO / FIXME / HACK
                    if content.contains("TODO") || content.contains("FIXME") || content.contains("HACK") {
                        new_comments.push((
                            ln,
                            DiffSide::Modified,
                            ReviewTag::Suggestion,
                            "Unresolved TODO/FIXME marker: resolve or track in issue tracker before merging."
                                .to_string(),
                        ));
                    }

                    // 3. Check for debug prints
                    if content.contains("println!") || content.contains("console.log(") || content.contains("dbg!") {
                        new_comments.push((
                            ln,
                            DiffSide::Modified,
                            ReviewTag::Nit,
                            "Leftover debug print detected: consider using structured log framework."
                                .to_string(),
                        ));
                    }

                    // 4. Check for potential hardcoded secret or token
                    let lower = content.to_lowercase();
                    if (lower.contains("secret") || lower.contains("api_key") || lower.contains("password"))
                        && (lower.contains('=') || lower.contains(':'))
                        && !lower.contains("env::")
                        && !lower.contains("getenv")
                    {
                        new_comments.push((
                            ln,
                            DiffSide::Modified,
                            ReviewTag::Issue,
                            "Security warning: possible hardcoded credential/secret; load from environment variables."
                                .to_string(),
                        ));
                        issues_found += 1;
                    }
                }
            }
        }

        if new_comments.is_empty() {
            self.add_comment(
                1,
                DiffSide::Modified,
                ReviewTag::Approval,
                "AI Review: clean changes detected. No panics, secrets, or syntax issues found."
                    .to_string(),
                "AI Review Engine".to_string(),
            );
            self.review_verdict = ReviewVerdict::Approved;
        } else {
            for (ln, side, tag, text) in new_comments {
                self.add_comment(ln, side, tag, text, "AI Review Engine".to_string());
            }
            if issues_found > 0 {
                self.review_verdict = ReviewVerdict::ChangesRequested;
            } else {
                self.review_verdict = ReviewVerdict::InReview;
            }
        }

        self.review_mode = true;
        self.comments.len()
    }

    /// Number of total comments.
    #[must_use]
    pub fn comment_count(&self) -> usize {
        self.comments.len()
    }

    /// Number of resolved comments.
    #[must_use]
    pub fn resolved_comment_count(&self) -> usize {
        self.comments.iter().filter(|c| c.resolved).count()
    }

    /// Generates markdown export for PR review or clipboard sharing.
    #[must_use]
    pub fn generate_review_markdown(&self) -> String {
        use std::fmt::Write;
        let mut md = format!("# Code Review: {}\n\n", self.file_name);
        let _ = writeln!(md, "**Folder:** `{}`  ", self.path_info);
        let _ = writeln!(md, "**Status:** {}  ", self.review_verdict.badge_text());
        let _ = writeln!(
            md,
            "**Progress:** {} / {} comments resolved\n",
            self.resolved_comment_count(),
            self.comment_count()
        );
        md.push_str("### Inline Review Notes\n\n");
        if self.comments.is_empty() {
            md.push_str("_No review comments. Changes look ready to merge!_\n");
        } else {
            for c in &self.comments {
                let side_label = match c.side {
                    DiffSide::Original => "Original",
                    DiffSide::Modified => "Modified",
                };
                let status_badge = if c.resolved { "[RESOLVED]" } else { "[OPEN]" };
                let _ = writeln!(
                    md,
                    "- **Line {} ({})** `{}` {} {}\n  _Reviewer: {}_\n",
                    c.line_number,
                    side_label,
                    c.tag.label(),
                    status_badge,
                    c.content,
                    c.author
                );
            }
        }
        md
    }
}

impl Default for DiffViewState {
    #[allow(clippy::too_many_lines)]
    fn default() -> Self {
        Self {
            file_name: "sw.js".to_string(),
            path_info: "public/".to_string(),
            hunks: vec![DiffHunk {
                header: "@@ -1,9 +1,10 @@".to_string(),
                left_lines: vec![
                    DiffLine {
                        line_number: Some(1),
                        content: "const CACHE_NAME = 'v1.0.4';".to_string(),
                        line_type: DiffLineType::Deletion,
                    },
                    DiffLine {
                        line_number: Some(2),
                        content: "const STATIC_ASSETS = [".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(3),
                        content: "  '/',".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(4),
                        content: "  '/index.html',".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(5),
                        content: "  '/bundle.js',".to_string(),
                        line_type: DiffLineType::Deletion,
                    },
                    DiffLine {
                        line_number: None,
                        content: String::new(),
                        line_type: DiffLineType::Empty,
                    },
                    DiffLine {
                        line_number: Some(6),
                        content: "];".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(7),
                        content: "self.addEventListener('install', (e) => {".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(8),
                        content: "  e.waitUntil(caches.open(CACHE_NAME));".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(9),
                        content: "});".to_string(),
                        line_type: DiffLineType::Context,
                    },
                ],
                right_lines: vec![
                    DiffLine {
                        line_number: Some(1),
                        content: "const CACHE_NAME = 'v1.0.5-beta';".to_string(),
                        line_type: DiffLineType::Addition,
                    },
                    DiffLine {
                        line_number: Some(2),
                        content: "const STATIC_ASSETS = [".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(3),
                        content: "  '/',".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(4),
                        content: "  '/index.html',".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(5),
                        content: "  '/bundle.min.js',".to_string(),
                        line_type: DiffLineType::Addition,
                    },
                    DiffLine {
                        line_number: Some(6),
                        content: "  '/vendor.wasm',".to_string(),
                        line_type: DiffLineType::Addition,
                    },
                    DiffLine {
                        line_number: Some(7),
                        content: "];".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(8),
                        content: "self.addEventListener('install', (e) => {".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(9),
                        content: "  e.waitUntil(caches.open(CACHE_NAME));".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(10),
                        content: "});".to_string(),
                        line_type: DiffLineType::Context,
                    },
                ],
            }],
            scroll_offset: 0,
            staged: true,
            review_mode: true,
            review_verdict: ReviewVerdict::InReview,
            comments: vec![
                ReviewComment {
                    id: 1,
                    line_number: 1,
                    side: DiffSide::Modified,
                    author: "Reviewer".to_string(),
                    tag: ReviewTag::Suggestion,
                    content: "Consider pinning release version instead of -beta".to_string(),
                    resolved: false,
                },
                ReviewComment {
                    id: 2,
                    line_number: 6,
                    side: DiffSide::Modified,
                    author: "Reviewer".to_string(),
                    tag: ReviewTag::Approval,
                    content: "Good addition: vendor.wasm properly bundled".to_string(),
                    resolved: true,
                },
            ],
            next_comment_id: 3,
            selected_line: None,
            file_index: 0,
            total_files: 1,
            comment_draft: None,
        }
    }
}

/// Interactive actions dispatched by clicking inside the diff view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffAction {
    /// No interactive control was clicked.
    None,
    /// Open the current file in external editor.
    OpenFile,
    /// Stage or unstage the current file.
    ToggleStage,
    /// Restore / discard changes to the current file.
    Restore,
    /// Close the diff view.
    Close,
    /// Toggle between standard Diff and Code Review toolbar.
    ToggleReviewMode,
    /// Trigger an automated AI code review pass over changes.
    RunAiReview,
    /// Update the review verdict (`InReview`, `Approved`, `ChangesRequested`).
    SetVerdict(ReviewVerdict),
    /// Cycle the review verdict.
    CycleVerdict,
    /// Toggle resolved state of a comment by ID.
    ToggleResolve(usize),
    /// Delete a comment by ID.
    DeleteComment(usize),
    /// Copy review markdown summary to clipboard.
    CopyReviewSummary,
    /// Click on a diff line to select or review it.
    SelectLine(usize, DiffSide),
    /// Switch to previous changed file in review queue.
    PrevFile,
    /// Switch to next changed file in review queue.
    NextFile,
    /// Set tag for active comment draft.
    SetDraftTag(ReviewTag),
    /// Submit active comment draft.
    SubmitDraft,
    /// Cancel active comment draft.
    CancelDraft,
}

/// Precomputed button and tab geometries for the diff header bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffHeaderLayout {
    pub is_compact: bool,
    pub is_tight: bool,
    pub file_title_rect: (f32, f32, f32, f32),
    pub prev_file_rect: Option<(f32, f32, f32, f32)>,
    pub file_counter_rect: Option<(f32, f32, f32, f32)>,
    pub next_file_rect: Option<(f32, f32, f32, f32)>,
    pub diff_tab_rect: (f32, f32, f32, f32),
    pub review_tab_rect: (f32, f32, f32, f32),
    pub verdict_badge_rect: Option<(f32, f32, f32, f32)>,
    pub ai_review_btn_rect: Option<(f32, f32, f32, f32)>,
    pub approve_btn_rect: Option<(f32, f32, f32, f32)>,
    pub changes_btn_rect: Option<(f32, f32, f32, f32)>,
    pub export_btn_rect: Option<(f32, f32, f32, f32)>,
    pub stage_btn_rect: (f32, f32, f32, f32),
    pub open_btn_rect: (f32, f32, f32, f32),
    pub close_btn_rect: (f32, f32, f32, f32),
}

impl DiffHeaderLayout {
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn compute(diff_x: f32, diff_y: f32, diff_width: f32, state: &DiffViewState) -> Self {
        let btn_y = diff_y + 5.0;
        let btn_h = 24.0;

        // 1. Left side items: file title, multi-file switcher, diff/review tabs, verdict, and AI review button
        let title_max_w = if diff_width > 800.0 {
            100.0
        } else if diff_width > 550.0 {
            70.0
        } else {
            44.0
        };
        let file_title_rect = (diff_x + 8.0, diff_y + 8.0, title_max_w, 20.0);
        let mut next_x = diff_x + 8.0 + title_max_w + 6.0;

        let (prev_file_rect, file_counter_rect, next_file_rect) = if state.total_files > 1 {
            let p_rect = (next_x, btn_y, 20.0, btn_h);
            let c_rect = (next_x + 22.0, btn_y, 34.0, btn_h);
            let n_rect = (next_x + 58.0, btn_y, 20.0, btn_h);
            next_x += 82.0;
            (Some(p_rect), Some(c_rect), Some(n_rect))
        } else {
            (None, None, None)
        };

        let diff_tab_w = 42.0;
        let diff_tab_rect = (next_x, btn_y, diff_tab_w, btn_h);
        next_x += diff_tab_w + 4.0;

        let review_tab_w = 68.0;
        let review_tab_rect = (next_x, btn_y, review_tab_w, btn_h);
        next_x += review_tab_w + 4.0;

        let (verdict_badge_rect, ai_review_btn_rect) = if state.review_mode {
            let vb_w = 78.0;
            let vb = (next_x, btn_y, vb_w, btn_h);
            next_x += vb_w + 4.0;

            let ai_w = 84.0;
            let ai_rect = (next_x, btn_y, ai_w, btn_h);
            next_x += ai_w + 6.0;

            (Some(vb), Some(ai_rect))
        } else {
            (None, None)
        };

        let left_boundary = next_x;
        let right_budget = (diff_x + diff_width - left_boundary).max(0.0);
        let is_compact = right_budget < 300.0;
        let is_tight = right_budget < 170.0;

        // 2. Right side buttons dynamically allocated to prevent overlap
        let close_w = 22.0;
        let close_x = (diff_x + diff_width - close_w - 6.0).max(left_boundary);

        let open_w = if is_tight { 24.0 } else if is_compact { 48.0 } else { 76.0 };
        let open_x = (close_x - open_w - 4.0).max(left_boundary);

        let stage_w = if is_tight { 24.0 } else if is_compact { 48.0 } else { 72.0 };
        let stage_x = (open_x - stage_w - 4.0).max(left_boundary);

        let (export_btn_rect, changes_btn_rect, approve_btn_rect) = if state.review_mode {
            let exp_w = if is_tight { 24.0 } else if is_compact { 46.0 } else { 68.0 };
            let exp_x = (stage_x - exp_w - 4.0).max(left_boundary);

            let chg_w = if is_tight { 24.0 } else if is_compact { 52.0 } else { 76.0 };
            let chg_x = (exp_x - chg_w - 4.0).max(left_boundary);

            let app_w = if is_tight { 24.0 } else if is_compact { 52.0 } else { 76.0 };
            let app_x = (chg_x - app_w - 4.0).max(left_boundary);

            (
                Some((exp_x, btn_y, exp_w, btn_h)),
                Some((chg_x, btn_y, chg_w, btn_h)),
                Some((app_x, btn_y, app_w, btn_h)),
            )
        } else {
            (None, None, None)
        };

        Self {
            is_compact,
            is_tight,
            file_title_rect,
            prev_file_rect,
            file_counter_rect,
            next_file_rect,
            diff_tab_rect,
            review_tab_rect,
            verdict_badge_rect,
            ai_review_btn_rect,
            approve_btn_rect,
            changes_btn_rect,
            export_btn_rect,
            stage_btn_rect: (stage_x, btn_y, stage_w, btn_h),
            open_btn_rect: (open_x, btn_y, open_w, btn_h),
            close_btn_rect: (close_x, btn_y, close_w, btn_h),
        }
    }

    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32, diff_y: f32) -> DiffAction {
        if y < diff_y || y > diff_y + DIFF_HEADER_HEIGHT {
            return DiffAction::None;
        }

        let in_box = |rect: (f32, f32, f32, f32)| x >= rect.0 && x <= rect.0 + rect.2;

        if in_box(self.close_btn_rect) {
            return DiffAction::Close;
        }
        if let Some(r) = self.prev_file_rect
            && in_box(r)
        {
            return DiffAction::PrevFile;
        }
        if let Some(r) = self.next_file_rect
            && in_box(r)
        {
            return DiffAction::NextFile;
        }
        if let Some(r) = self.approve_btn_rect
            && in_box(r)
        {
            return DiffAction::SetVerdict(ReviewVerdict::Approved);
        }
        if let Some(r) = self.changes_btn_rect
            && in_box(r)
        {
            return DiffAction::SetVerdict(ReviewVerdict::ChangesRequested);
        }
        if let Some(r) = self.export_btn_rect
            && in_box(r)
        {
            return DiffAction::CopyReviewSummary;
        }
        if let Some(r) = self.ai_review_btn_rect
            && in_box(r)
        {
            return DiffAction::RunAiReview;
        }
        if in_box(self.stage_btn_rect) {
            return DiffAction::ToggleStage;
        }
        if in_box(self.open_btn_rect) {
            return DiffAction::OpenFile;
        }
        if let Some(r) = self.verdict_badge_rect
            && in_box(r)
        {
            return DiffAction::CycleVerdict;
        }
        if in_box(self.diff_tab_rect) {
            return DiffAction::ToggleReviewMode;
        }
        if in_box(self.review_tab_rect) {
            return DiffAction::ToggleReviewMode;
        }

        DiffAction::None
    }
}

/// Helper to push a rectangular button quad with a 1px border.
fn push_button_quads(
    quads: &mut Vec<QuadVertex>,
    rect: (f32, f32, f32, f32),
    bg: [f32; 4],
    border: [f32; 4],
) {
    quads.extend_from_slice(&build_quad_vertices(rect.0, rect.1, rect.2, rect.3, bg));
    quads.extend_from_slice(&build_quad_vertices(rect.0, rect.1, rect.2, 1.0, border));
    quads.extend_from_slice(&build_quad_vertices(
        rect.0,
        rect.1 + rect.3 - 1.0,
        rect.2,
        1.0,
        border,
    ));
    quads.extend_from_slice(&build_quad_vertices(rect.0, rect.1, 1.0, rect.3, border));
    quads.extend_from_slice(&build_quad_vertices(
        rect.0 + rect.2 - 1.0,
        rect.1,
        1.0,
        rect.3,
        border,
    ));
}

/// Builds quads for the side-by-side diff: pitch black background, column gutters,
/// center divider, green/red line highlights, and inline code review comment cards.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::if_not_else
)]
pub fn assemble_diff_view_quads(
    diff_x: f32,
    diff_y: f32,
    diff_width: f32,
    diff_height: f32,
    accent_color: [f32; 4],
    state: &DiffViewState,
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();

    // 1. Pure pitch black workspace background (#000000)
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        diff_y,
        diff_width,
        diff_height,
        [0.0, 0.0, 0.0, 1.0],
    ));

    // 2. Left 1px border dividing from terminal / agent pane (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        diff_y,
        1.0,
        diff_height,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // 3. Top header bar background and bottom 1px divider (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        diff_y,
        diff_width,
        DIFF_HEADER_HEIGHT,
        [0.025, 0.028, 0.038, 0.95],
    ));
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        diff_y + DIFF_HEADER_HEIGHT - 1.0,
        diff_width,
        1.0,
        [0.095, 0.105, 0.13, 1.0],
    ));

    let layout = DiffHeaderLayout::compute(diff_x, diff_y, diff_width, state);

    // Multi-file switcher buttons (< 1/3 >)
    if let (Some(p), Some(c), Some(n)) = (
        layout.prev_file_rect,
        layout.file_counter_rect,
        layout.next_file_rect,
    ) {
        push_button_quads(
            &mut quads,
            p,
            [0.07, 0.09, 0.13, 0.95],
            [0.22, 0.28, 0.38, 0.9],
        );
        quads.extend_from_slice(&build_quad_vertices(
            c.0,
            c.1,
            c.2,
            c.3,
            [0.03, 0.04, 0.06, 0.8],
        ));
        push_button_quads(
            &mut quads,
            n,
            [0.07, 0.09, 0.13, 0.95],
            [0.22, 0.28, 0.38, 0.9],
        );
    }

    // Active mode tab underline indicator
    let (underline_x, underline_w) = if state.review_mode {
        (layout.review_tab_rect.0, layout.review_tab_rect.2)
    } else {
        (layout.diff_tab_rect.0, layout.diff_tab_rect.2)
    };
    quads.extend_from_slice(&build_quad_vertices(
        underline_x,
        diff_y + DIFF_HEADER_HEIGHT - 2.0,
        underline_w,
        2.0,
        accent_color,
    ));

    // Diff / Review mode toggle buttons
    let diff_btn_bg = if !state.review_mode {
        [0.09, 0.11, 0.16, 0.95]
    } else {
        [0.04, 0.05, 0.07, 0.8]
    };
    let diff_border = if !state.review_mode {
        accent_color
    } else {
        [0.18, 0.20, 0.26, 0.7]
    };
    push_button_quads(&mut quads, layout.diff_tab_rect, diff_btn_bg, diff_border);

    let review_btn_bg = if state.review_mode {
        [0.09, 0.11, 0.16, 0.95]
    } else {
        [0.04, 0.05, 0.07, 0.8]
    };
    let review_border = if state.review_mode {
        accent_color
    } else {
        [0.18, 0.20, 0.26, 0.7]
    };
    push_button_quads(
        &mut quads,
        layout.review_tab_rect,
        review_btn_bg,
        review_border,
    );

    // If review mode is active, render review verdict badge and review actions
    if let Some(vb) = layout.verdict_badge_rect {
        let v_color = state.review_verdict.badge_color();
        // Verdict pill badge
        push_button_quads(
            &mut quads,
            vb,
            [
                v_color[0] * 0.24,
                v_color[1] * 0.24,
                v_color[2] * 0.24,
                0.95,
            ],
            [
                v_color[0] * 0.65,
                v_color[1] * 0.65,
                v_color[2] * 0.65,
                0.85,
            ],
        );
        quads.extend_from_slice(&build_quad_vertices(vb.0, vb.1, 3.0, vb.3, v_color));
    }

    if let Some(ai) = layout.ai_review_btn_rect {
        push_button_quads(
            &mut quads,
            ai,
            [0.10, 0.14, 0.22, 0.95],
            [0.32, 0.52, 0.90, 0.9],
        );
        quads.extend_from_slice(&build_quad_vertices(
            ai.0,
            ai.1,
            2.5,
            ai.3,
            [0.35, 0.75, 1.0, 1.0],
        ));
    }

    if let Some(app) = layout.approve_btn_rect {
        push_button_quads(
            &mut quads,
            app,
            [0.08, 0.20, 0.12, 0.95],
            [0.20, 0.65, 0.35, 0.9],
        );
    }

    if let Some(chg) = layout.changes_btn_rect {
        push_button_quads(
            &mut quads,
            chg,
            [0.24, 0.10, 0.12, 0.95],
            [0.70, 0.25, 0.28, 0.9],
        );
    }

    if let Some(exp) = layout.export_btn_rect {
        push_button_quads(
            &mut quads,
            exp,
            [0.08, 0.10, 0.15, 0.95],
            [0.25, 0.38, 0.55, 0.9],
        );
    }

    // Stage button
    push_button_quads(
        &mut quads,
        layout.stage_btn_rect,
        [0.07, 0.09, 0.13, 0.95],
        [0.22, 0.28, 0.38, 0.9],
    );
    // Open File button
    push_button_quads(
        &mut quads,
        layout.open_btn_rect,
        [0.07, 0.09, 0.13, 0.95],
        [0.22, 0.28, 0.38, 0.9],
    );
    // Close button
    push_button_quads(
        &mut quads,
        layout.close_btn_rect,
        [0.15, 0.08, 0.09, 0.95],
        [0.45, 0.20, 0.22, 0.9],
    );

    // 4. Side-by-side columns geometry
    let content_y = diff_y + DIFF_HEADER_HEIGHT;
    let content_h = (diff_height - DIFF_HEADER_HEIGHT).max(0.0);
    let half_w = (diff_width / 2.0).floor();

    // Center vertical split line
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + half_w,
        content_y,
        1.0,
        content_h,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // Left gutter background
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        content_y,
        DIFF_GUTTER_WIDTH,
        content_h,
        [0.015, 0.018, 0.024, 0.95],
    ));
    // Left gutter divider
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + DIFF_GUTTER_WIDTH,
        content_y,
        1.0,
        content_h,
        [0.07, 0.075, 0.095, 0.8],
    ));

    // Right gutter background
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + half_w,
        content_y,
        DIFF_GUTTER_WIDTH,
        content_h,
        [0.015, 0.018, 0.024, 0.95],
    ));
    // Right gutter divider
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + half_w + DIFF_GUTTER_WIDTH,
        content_y,
        1.0,
        content_h,
        [0.07, 0.075, 0.095, 0.8],
    ));

    // 5. Line highlights and inline code review cards
    let mut current_y = content_y;
    for hunk in &state.hunks {
        // Hunk header bar (subtle dark navy strip)
        quads.extend_from_slice(&build_quad_vertices(
            diff_x,
            current_y,
            diff_width,
            DIFF_LINE_HEIGHT,
            [0.04, 0.05, 0.07, 0.75],
        ));
        current_y += DIFF_LINE_HEIGHT;

        let max_lines = hunk.left_lines.len().max(hunk.right_lines.len());
        for line_idx in 0..max_lines {
            if current_y + DIFF_LINE_HEIGHT > diff_y + diff_height {
                break;
            }

            let left_line = hunk.left_lines.get(line_idx);
            let right_line = hunk.right_lines.get(line_idx);

            // Left line highlight
            if let Some(left) = left_line
                && left.line_type == DiffLineType::Deletion
            {
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + DIFF_GUTTER_WIDTH + 1.0,
                    current_y,
                    half_w - DIFF_GUTTER_WIDTH - 1.0,
                    DIFF_LINE_HEIGHT,
                    [0.169, 0.067, 0.078, 0.85],
                ));
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x,
                    current_y,
                    DIFF_GUTTER_WIDTH,
                    DIFF_LINE_HEIGHT,
                    [0.294, 0.09, 0.11, 0.8],
                ));
            }

            // Right line highlight
            if let Some(right) = right_line
                && right.line_type == DiffLineType::Addition
            {
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + half_w + DIFF_GUTTER_WIDTH + 1.0,
                    current_y,
                    half_w - DIFF_GUTTER_WIDTH - 1.0,
                    DIFF_LINE_HEIGHT,
                    [0.063, 0.153, 0.098, 0.85],
                ));
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + half_w,
                    current_y,
                    DIFF_GUTTER_WIDTH,
                    DIFF_LINE_HEIGHT,
                    [0.09, 0.27, 0.16, 0.8],
                ));
            }

            current_y += DIFF_LINE_HEIGHT;

            // 5.5 Inline Review Comment Cards (if review mode is active)
            if state.review_mode {
                let left_comments: Vec<&ReviewComment> = left_line
                    .and_then(|l| l.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Original && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let right_comments: Vec<&ReviewComment> = right_line
                    .and_then(|r| r.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Modified && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let max_comments = left_comments.len().max(right_comments.len());
                for c_idx in 0..max_comments {
                    if current_y + DIFF_COMMENT_HEIGHT > diff_y + diff_height {
                        break;
                    }

                    if let Some(c) = left_comments.get(c_idx) {
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                        let bg_color = if c.resolved {
                            [0.04, 0.05, 0.06, 0.85]
                        } else {
                            [0.08, 0.09, 0.13, 0.95]
                        };
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            card_w,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            bg_color,
                        ));
                        // Left accent bar
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            3.0,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            c.tag.color(),
                        ));
                        // Resolve button
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x + card_w - 42.0,
                            current_y + 3.0,
                            18.0,
                            16.0,
                            [0.12, 0.14, 0.18, 0.9],
                        ));
                        // Delete button
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x + card_w - 20.0,
                            current_y + 3.0,
                            16.0,
                            16.0,
                            [0.12, 0.14, 0.18, 0.9],
                        ));
                    }

                    if let Some(c) = right_comments.get(c_idx) {
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                        let bg_color = if c.resolved {
                            [0.04, 0.05, 0.06, 0.85]
                        } else {
                            [0.08, 0.09, 0.13, 0.95]
                        };
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            card_w,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            bg_color,
                        ));
                        // Left accent bar
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            3.0,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            c.tag.color(),
                        ));
                        // Resolve button
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x + card_w - 42.0,
                            current_y + 3.0,
                            18.0,
                            16.0,
                            [0.12, 0.14, 0.18, 0.9],
                        ));
                        // Delete button
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x + card_w - 20.0,
                            current_y + 3.0,
                            16.0,
                            16.0,
                            [0.12, 0.14, 0.18, 0.9],
                        ));
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }

                // Render in-progress draft comment card if attached to this line
                let has_left_draft = left_line.and_then(|l| l.line_number).is_some_and(|ln| {
                    state
                        .comment_draft
                        .as_ref()
                        .is_some_and(|d| d.side == DiffSide::Original && d.line_number == ln)
                });

                let has_right_draft = right_line.and_then(|r| r.line_number).is_some_and(|ln| {
                    state
                        .comment_draft
                        .as_ref()
                        .is_some_and(|d| d.side == DiffSide::Modified && d.line_number == ln)
                });

                if (has_left_draft || has_right_draft)
                    && current_y + DIFF_COMMENT_HEIGHT <= diff_y + diff_height
                {
                    if has_left_draft && let Some(draft) = &state.comment_draft {
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                        push_button_quads(
                            &mut quads,
                            (card_x, current_y + 1.0, card_w, DIFF_COMMENT_HEIGHT - 2.0),
                            [0.08, 0.10, 0.15, 0.98],
                            accent_color,
                        );
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            3.0,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            draft.tag.color(),
                        ));
                        push_button_quads(
                            &mut quads,
                            (card_x + 6.0, current_y + 3.0, 56.0, 16.0),
                            [0.12, 0.15, 0.22, 0.9],
                            draft.tag.color(),
                        );
                        push_button_quads(
                            &mut quads,
                            (card_x + card_w - 64.0, current_y + 3.0, 38.0, 16.0),
                            [0.10, 0.24, 0.15, 0.95],
                            [0.25, 0.70, 0.40, 0.9],
                        );
                        push_button_quads(
                            &mut quads,
                            (card_x + card_w - 22.0, current_y + 3.0, 18.0, 16.0),
                            [0.20, 0.08, 0.10, 0.95],
                            [0.60, 0.20, 0.22, 0.9],
                        );
                    }

                    if has_right_draft && let Some(draft) = &state.comment_draft {
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                        push_button_quads(
                            &mut quads,
                            (card_x, current_y + 1.0, card_w, DIFF_COMMENT_HEIGHT - 2.0),
                            [0.08, 0.10, 0.15, 0.98],
                            accent_color,
                        );
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            3.0,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            draft.tag.color(),
                        ));
                        push_button_quads(
                            &mut quads,
                            (card_x + 6.0, current_y + 3.0, 56.0, 16.0),
                            [0.12, 0.15, 0.22, 0.9],
                            draft.tag.color(),
                        );
                        push_button_quads(
                            &mut quads,
                            (card_x + card_w - 64.0, current_y + 3.0, 38.0, 16.0),
                            [0.10, 0.24, 0.15, 0.95],
                            [0.25, 0.70, 0.40, 0.9],
                        );
                        push_button_quads(
                            &mut quads,
                            (card_x + card_w - 22.0, current_y + 3.0, 18.0, 16.0),
                            [0.20, 0.08, 0.10, 0.95],
                            [0.60, 0.20, 0.22, 0.9],
                        );
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }
            }
        }
    }

    quads
}

/// Builds text labels for the diff header, review toolbar, line numbers, code, and comments.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::if_not_else
)]
pub fn build_diff_view_labels(
    diff_x: f32,
    diff_y: f32,
    diff_width: f32,
    diff_height: f32,
    state: &DiffViewState,
) -> Vec<TabLabel> {
    let mut labels = Vec::new();

    let layout = DiffHeaderLayout::compute(diff_x, diff_y, diff_width, state);

    // 1. File Title
    let tab_text = format!("{} {}", state.file_name, state.path_info);
    labels.push(TabLabel {
        text: tab_text,
        x: layout.file_title_rect.0,
        y: layout.file_title_rect.1,
        color: GlyphonColor::rgb(240, 245, 255),
        bold: true,
        max_width: layout.file_title_rect.2,
    });

    let btn_text_y = diff_y + 9.0;

    // Diff / Review mode toggle labels
    let diff_tab_text = "Diff";
    labels.push(TabLabel {
        text: diff_tab_text.to_string(),
        x: layout.diff_tab_rect.0 + 8.0,
        y: btn_text_y,
        color: if !state.review_mode {
            GlyphonColor::rgb(240, 245, 255)
        } else {
            GlyphonColor::rgb(140, 145, 160)
        },
        bold: !state.review_mode,
        max_width: (layout.diff_tab_rect.2 - 6.0).max(10.0),
    });

    let review_text = format!("Review ({})", state.comment_count());
    labels.push(TabLabel {
        text: review_text,
        x: layout.review_tab_rect.0 + 6.0,
        y: btn_text_y,
        color: if state.review_mode {
            GlyphonColor::rgb(240, 245, 255)
        } else {
            GlyphonColor::rgb(140, 145, 160)
        },
        bold: state.review_mode,
        max_width: (layout.review_tab_rect.2 - 6.0).max(10.0),
    });

    // Review status and actions if review mode active
    if let Some(vb) = layout.verdict_badge_rect {
        let v_color = state.review_verdict.badge_color();
        let vr = (v_color[0] * 255.0).round() as u8;
        let vg = (v_color[1] * 255.0).round() as u8;
        let vb_b = (v_color[2] * 255.0).round() as u8;

        let badge_text = state.review_verdict.badge_text();

        labels.push(TabLabel {
            text: badge_text.to_string(),
            x: vb.0 + 6.0,
            y: btn_text_y,
            color: GlyphonColor::rgb(vr, vg, vb_b),
            bold: true,
            max_width: (vb.2 - 8.0).max(10.0),
        });
    }

    if let Some(ai) = layout.ai_review_btn_rect {
        let ai_text = if layout.is_compact { "⚡ AI" } else { "⚡ AI Review" };
        labels.push(TabLabel {
            text: ai_text.to_string(),
            x: ai.0 + 6.0,
            y: btn_text_y,
            color: GlyphonColor::rgb(140, 215, 255),
            bold: true,
            max_width: (ai.2 - 8.0).max(10.0),
        });
    }

    if let Some(app) = layout.approve_btn_rect {
        let app_text = if layout.is_tight {
            "✓"
        } else if layout.is_compact {
            "Appr"
        } else {
            "Approve"
        };
        let offset_x = if layout.is_tight { 6.0 } else { 8.0 };
        labels.push(TabLabel {
            text: app_text.to_string(),
            x: app.0 + offset_x,
            y: btn_text_y,
            color: GlyphonColor::rgb(130, 230, 160),
            bold: true,
            max_width: (app.2 - 4.0).max(10.0),
        });
    }

    if let Some(chg) = layout.changes_btn_rect {
        let chg_text = if layout.is_tight {
            "✗"
        } else if layout.is_compact {
            "Chgs"
        } else {
            "Changes"
        };
        let offset_x = if layout.is_tight { 6.0 } else { 8.0 };
        labels.push(TabLabel {
            text: chg_text.to_string(),
            x: chg.0 + offset_x,
            y: btn_text_y,
            color: GlyphonColor::rgb(255, 140, 150),
            bold: true,
            max_width: (chg.2 - 4.0).max(10.0),
        });
    }

    if let Some(exp) = layout.export_btn_rect {
        let exp_text = if layout.is_tight {
            "Exp"
        } else if layout.is_compact {
            "Exp"
        } else {
            "Export"
        };
        let offset_x = if layout.is_tight { 4.0 } else { 8.0 };
        labels.push(TabLabel {
            text: exp_text.to_string(),
            x: exp.0 + offset_x,
            y: btn_text_y,
            color: GlyphonColor::rgb(210, 215, 230),
            bold: true,
            max_width: (exp.2 - 4.0).max(10.0),
        });
    }

    // Stage button
    let stage_text = if layout.is_tight {
        if state.staged { "-" } else { "+" }
    } else if layout.is_compact {
        if state.staged { "Unstage" } else { "Stage" }
    } else if state.staged {
        "Unstage"
    } else {
        "Stage"
    };
    let stage_offset = if layout.is_tight { 6.0 } else { 8.0 };
    labels.push(TabLabel {
        text: stage_text.to_string(),
        x: layout.stage_btn_rect.0 + stage_offset,
        y: btn_text_y,
        color: GlyphonColor::rgb(220, 225, 240),
        bold: false,
        max_width: (layout.stage_btn_rect.2 - 4.0).max(10.0),
    });

    // Open File button
    let open_text = if layout.is_tight {
        "↗"
    } else if layout.is_compact {
        "Open"
    } else {
        "Open File"
    };
    let open_offset = if layout.is_tight { 6.0 } else { 8.0 };
    labels.push(TabLabel {
        text: open_text.to_string(),
        x: layout.open_btn_rect.0 + open_offset,
        y: btn_text_y,
        color: GlyphonColor::rgb(220, 225, 240),
        bold: false,
        max_width: (layout.open_btn_rect.2 - 4.0).max(10.0),
    });

    // Close 'x' button
    labels.push(TabLabel {
        text: "x".to_string(),
        x: layout.close_btn_rect.0 + 7.0,
        y: btn_text_y,
        color: GlyphonColor::rgb(160, 165, 180),
        bold: true,
        max_width: (layout.close_btn_rect.2 - 4.0).max(10.0),
    });

    // Multi-file switcher labels (< 1/3 >)
    if let (Some(p), Some(c), Some(n)) = (
        layout.prev_file_rect,
        layout.file_counter_rect,
        layout.next_file_rect,
    ) {
        labels.push(TabLabel {
            text: "<".to_string(),
            x: p.0 + 7.0,
            y: btn_text_y,
            color: if state.file_index > 0 {
                GlyphonColor::rgb(220, 225, 240)
            } else {
                GlyphonColor::rgb(90, 95, 110)
            },
            bold: true,
            max_width: (p.2 - 4.0).max(10.0),
        });
        labels.push(TabLabel {
            text: format!("{}/{}", state.file_index + 1, state.total_files),
            x: c.0 + 6.0,
            y: btn_text_y,
            color: GlyphonColor::rgb(180, 185, 200),
            bold: false,
            max_width: (c.2 - 4.0).max(10.0),
        });
        labels.push(TabLabel {
            text: ">".to_string(),
            x: n.0 + 7.0,
            y: btn_text_y,
            color: if state.file_index + 1 < state.total_files {
                GlyphonColor::rgb(220, 225, 240)
            } else {
                GlyphonColor::rgb(90, 95, 110)
            },
            bold: true,
            max_width: (n.2 - 4.0).max(10.0),
        });
    }

    // 2. Diff Lines and Review Comments Rendering
    let content_y = diff_y + DIFF_HEADER_HEIGHT;
    let half_w = (diff_width / 2.0).floor();
    let mut current_y = content_y;

    if state.hunks.is_empty() {
        labels.push(TabLabel {
            text: "No diff hunks or modifications in this commit / file.".to_string(),
            x: diff_x + 20.0,
            y: content_y + 30.0,
            color: GlyphonColor::rgb(140, 150, 175),
            bold: false,
            max_width: diff_width - 40.0,
        });
    }

    for hunk in &state.hunks {
        // Hunk header text e.g. @@ -1,9 +1,10 @@
        labels.push(TabLabel {
            text: hunk.header.clone(),
            x: diff_x + 10.0,
            y: current_y + 3.0,
            color: GlyphonColor::rgb(120, 150, 200),
            bold: true,
            max_width: diff_width - 20.0,
        });
        current_y += DIFF_LINE_HEIGHT;

        let max_lines = hunk.left_lines.len().max(hunk.right_lines.len());
        for line_idx in 0..max_lines {
            if current_y + DIFF_LINE_HEIGHT > diff_y + diff_height {
                break;
            }

            let left_line = hunk.left_lines.get(line_idx);
            let right_line = hunk.right_lines.get(line_idx);

            // Left line (Original)
            if let Some(left) = left_line {
                if let Some(num) = left.line_number {
                    labels.push(TabLabel {
                        text: format!("{num:>3}"),
                        x: diff_x + 8.0,
                        y: current_y + 3.0,
                        color: GlyphonColor::rgb(110, 115, 130),
                        bold: false,
                        max_width: DIFF_GUTTER_WIDTH - 10.0,
                    });
                }

                if !left.content.is_empty() {
                    let char_w = 7.22f32;
                    let mut col_offset = 0;
                    for span in
                        tokenize_syntax_line(&left.content, &state.file_name, left.line_type)
                    {
                        let span_len = span.text.chars().count();
                        let span_x =
                            diff_x + DIFF_GUTTER_WIDTH + 8.0 + (col_offset as f32 * char_w);
                        let max_w =
                            (half_w - DIFF_GUTTER_WIDTH - 16.0 - (col_offset as f32 * char_w))
                                .max(0.0);
                        if max_w > 0.0 {
                            labels.push(TabLabel {
                                text: span.text,
                                x: span_x,
                                y: current_y + 3.0,
                                color: span.color,
                                bold: span.bold,
                                max_width: max_w,
                            });
                        }
                        col_offset += span_len;
                    }
                }
            }

            // Right line (Modified)
            if let Some(right) = right_line {
                if let Some(num) = right.line_number {
                    labels.push(TabLabel {
                        text: format!("{num:>3}"),
                        x: diff_x + half_w + 8.0,
                        y: current_y + 3.0,
                        color: GlyphonColor::rgb(110, 115, 130),
                        bold: false,
                        max_width: DIFF_GUTTER_WIDTH - 10.0,
                    });
                }

                if !right.content.is_empty() {
                    let char_w = 7.22f32;
                    let mut col_offset = 0;
                    for span in
                        tokenize_syntax_line(&right.content, &state.file_name, right.line_type)
                    {
                        let span_len = span.text.chars().count();
                        let span_x = diff_x
                            + half_w
                            + DIFF_GUTTER_WIDTH
                            + 8.0
                            + (col_offset as f32 * char_w);
                        let max_w =
                            (half_w - DIFF_GUTTER_WIDTH - 16.0 - (col_offset as f32 * char_w))
                                .max(0.0);
                        if max_w > 0.0 {
                            labels.push(TabLabel {
                                text: span.text,
                                x: span_x,
                                y: current_y + 3.0,
                                color: span.color,
                                bold: span.bold,
                                max_width: max_w,
                            });
                        }
                        col_offset += span_len;
                    }
                }
            }

            current_y += DIFF_LINE_HEIGHT;

            // Render inline review comment labels
            if state.review_mode {
                let left_comments: Vec<&ReviewComment> = left_line
                    .and_then(|l| l.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Original && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let right_comments: Vec<&ReviewComment> = right_line
                    .and_then(|r| r.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Modified && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let max_comments = left_comments.len().max(right_comments.len());
                for c_idx in 0..max_comments {
                    if current_y + DIFF_COMMENT_HEIGHT > diff_y + diff_height {
                        break;
                    }

                    if let Some(c) = left_comments.get(c_idx) {
                        let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let tc = c.tag.color();
                        let tag_color = GlyphonColor::rgb(
                            (tc[0] * 255.0).round() as u8,
                            (tc[1] * 255.0).round() as u8,
                            (tc[2] * 255.0).round() as u8,
                        );

                        // Tag label
                        labels.push(TabLabel {
                            text: format!("[{}]", c.tag.label()),
                            x: card_x + 6.0,
                            y: current_y + 3.0,
                            color: tag_color,
                            bold: true,
                            max_width: 55.0,
                        });

                        // Comment content
                        let content_text = format!("{}: {}", c.author, c.content);
                        labels.push(TabLabel {
                            text: content_text,
                            x: card_x + 64.0,
                            y: current_y + 3.0,
                            color: if c.resolved {
                                GlyphonColor::rgb(140, 145, 155)
                            } else {
                                GlyphonColor::rgb(220, 225, 240)
                            },
                            bold: false,
                            max_width: card_w - 110.0,
                        });

                        // Resolve button
                        let resolve_char = if c.resolved { "[x]" } else { "[ ]" };
                        labels.push(TabLabel {
                            text: resolve_char.to_string(),
                            x: card_x + card_w - 42.0,
                            y: current_y + 3.0,
                            color: if c.resolved {
                                GlyphonColor::rgb(50, 210, 120)
                            } else {
                                GlyphonColor::rgb(150, 155, 170)
                            },
                            bold: true,
                            max_width: 22.0,
                        });

                        // Delete button
                        labels.push(TabLabel {
                            text: "x".to_string(),
                            x: card_x + card_w - 16.0,
                            y: current_y + 3.0,
                            color: GlyphonColor::rgb(160, 165, 180),
                            bold: true,
                            max_width: 14.0,
                        });
                    }

                    if let Some(c) = right_comments.get(c_idx) {
                        let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let tc = c.tag.color();
                        let tag_color = GlyphonColor::rgb(
                            (tc[0] * 255.0).round() as u8,
                            (tc[1] * 255.0).round() as u8,
                            (tc[2] * 255.0).round() as u8,
                        );

                        // Tag label
                        labels.push(TabLabel {
                            text: format!("[{}]", c.tag.label()),
                            x: card_x + 6.0,
                            y: current_y + 3.0,
                            color: tag_color,
                            bold: true,
                            max_width: 55.0,
                        });

                        // Comment content
                        let content_text = format!("{}: {}", c.author, c.content);
                        labels.push(TabLabel {
                            text: content_text,
                            x: card_x + 64.0,
                            y: current_y + 3.0,
                            color: if c.resolved {
                                GlyphonColor::rgb(140, 145, 155)
                            } else {
                                GlyphonColor::rgb(220, 225, 240)
                            },
                            bold: false,
                            max_width: card_w - 110.0,
                        });

                        // Resolve button
                        let resolve_char = if c.resolved { "[x]" } else { "[ ]" };
                        labels.push(TabLabel {
                            text: resolve_char.to_string(),
                            x: card_x + card_w - 42.0,
                            y: current_y + 3.0,
                            color: if c.resolved {
                                GlyphonColor::rgb(50, 210, 120)
                            } else {
                                GlyphonColor::rgb(150, 155, 170)
                            },
                            bold: true,
                            max_width: 22.0,
                        });

                        // Delete button
                        labels.push(TabLabel {
                            text: "x".to_string(),
                            x: card_x + card_w - 16.0,
                            y: current_y + 3.0,
                            color: GlyphonColor::rgb(160, 165, 180),
                            bold: true,
                            max_width: 14.0,
                        });
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }

                // Render in-progress draft comment labels if attached to this line
                let has_left_draft = left_line.and_then(|l| l.line_number).is_some_and(|ln| {
                    state
                        .comment_draft
                        .as_ref()
                        .is_some_and(|d| d.side == DiffSide::Original && d.line_number == ln)
                });

                let has_right_draft = right_line.and_then(|r| r.line_number).is_some_and(|ln| {
                    state
                        .comment_draft
                        .as_ref()
                        .is_some_and(|d| d.side == DiffSide::Modified && d.line_number == ln)
                });

                if (has_left_draft || has_right_draft)
                    && current_y + DIFF_COMMENT_HEIGHT <= diff_y + diff_height
                {
                    if has_left_draft && let Some(draft) = &state.comment_draft {
                        let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let tc = draft.tag.color();
                        let tag_color = GlyphonColor::rgb(
                            (tc[0] * 255.0).round() as u8,
                            (tc[1] * 255.0).round() as u8,
                            (tc[2] * 255.0).round() as u8,
                        );
                        labels.push(TabLabel {
                            text: format!("[{}]", draft.tag.label()),
                            x: card_x + 9.0,
                            y: current_y + 3.0,
                            color: tag_color,
                            bold: true,
                            max_width: 50.0,
                        });
                        let input_text = if draft.content.is_empty() {
                            "Type review comment (Enter to submit, Esc to cancel)..."
                        } else {
                            &draft.content
                        };
                        labels.push(TabLabel {
                            text: input_text.to_string(),
                            x: card_x + 66.0,
                            y: current_y + 3.0,
                            color: if draft.content.is_empty() {
                                GlyphonColor::rgb(130, 135, 150)
                            } else {
                                GlyphonColor::rgb(240, 245, 255)
                            },
                            bold: false,
                            max_width: (card_w - 136.0).max(10.0),
                        });
                        labels.push(TabLabel {
                            text: "Post".to_string(),
                            x: card_x + card_w - 58.0,
                            y: current_y + 3.0,
                            color: GlyphonColor::rgb(140, 235, 170),
                            bold: true,
                            max_width: 32.0,
                        });
                        labels.push(TabLabel {
                            text: "x".to_string(),
                            x: card_x + card_w - 18.0,
                            y: current_y + 3.0,
                            color: GlyphonColor::rgb(220, 140, 150),
                            bold: true,
                            max_width: 14.0,
                        });
                    }

                    if has_right_draft && let Some(draft) = &state.comment_draft {
                        let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let tc = draft.tag.color();
                        let tag_color = GlyphonColor::rgb(
                            (tc[0] * 255.0).round() as u8,
                            (tc[1] * 255.0).round() as u8,
                            (tc[2] * 255.0).round() as u8,
                        );
                        labels.push(TabLabel {
                            text: format!("[{}]", draft.tag.label()),
                            x: card_x + 9.0,
                            y: current_y + 3.0,
                            color: tag_color,
                            bold: true,
                            max_width: 50.0,
                        });
                        let input_text = if draft.content.is_empty() {
                            "Type review comment (Enter to submit, Esc to cancel)..."
                        } else {
                            &draft.content
                        };
                        labels.push(TabLabel {
                            text: input_text.to_string(),
                            x: card_x + 66.0,
                            y: current_y + 3.0,
                            color: if draft.content.is_empty() {
                                GlyphonColor::rgb(130, 135, 150)
                            } else {
                                GlyphonColor::rgb(240, 245, 255)
                            },
                            bold: false,
                            max_width: (card_w - 136.0).max(10.0),
                        });
                        labels.push(TabLabel {
                            text: "Post".to_string(),
                            x: card_x + card_w - 58.0,
                            y: current_y + 3.0,
                            color: GlyphonColor::rgb(140, 235, 170),
                            bold: true,
                            max_width: 32.0,
                        });
                        labels.push(TabLabel {
                            text: "x".to_string(),
                            x: card_x + card_w - 18.0,
                            y: current_y + 3.0,
                            color: GlyphonColor::rgb(220, 140, 150),
                            bold: true,
                            max_width: 14.0,
                        });
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }
            }
        }
    }

    labels
}

/// Hit tests mouse clicks within the diff view and review toolbar.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn hit_test_diff_view(
    x: f32,
    y: f32,
    diff_x: f32,
    diff_y: f32,
    diff_width: f32,
    diff_height: f32,
    state: &DiffViewState,
) -> DiffAction {
    if x < diff_x || x > diff_x + diff_width || y < diff_y || y > diff_y + diff_height {
        return DiffAction::None;
    }

    // 1. Check header bar buttons
    if y <= diff_y + DIFF_HEADER_HEIGHT {
        let layout = DiffHeaderLayout::compute(diff_x, diff_y, diff_width, state);
        let action = layout.hit_test(x, y, diff_y);
        if action != DiffAction::None {
            return action;
        }
        return DiffAction::None;
    }

    // 2. Check diff lines and inline comment card clicks
    let content_y = diff_y + DIFF_HEADER_HEIGHT;
    let half_w = (diff_width / 2.0).floor();
    let mut current_y = content_y;

    for hunk in &state.hunks {
        current_y += DIFF_LINE_HEIGHT; // hunk header

        let max_lines = hunk.left_lines.len().max(hunk.right_lines.len());
        for line_idx in 0..max_lines {
            if current_y + DIFF_LINE_HEIGHT > diff_y + diff_height {
                break;
            }

            let left_line = hunk.left_lines.get(line_idx);
            let right_line = hunk.right_lines.get(line_idx);

            // Check if click was on the line row itself
            if y >= current_y && y < current_y + DIFF_LINE_HEIGHT {
                if x < diff_x + half_w {
                    if let Some(left) = left_line
                        && let Some(num) = left.line_number
                    {
                        return DiffAction::SelectLine(num, DiffSide::Original);
                    }
                } else if let Some(right) = right_line
                    && let Some(num) = right.line_number
                {
                    return DiffAction::SelectLine(num, DiffSide::Modified);
                }
            }

            current_y += DIFF_LINE_HEIGHT;

            // Check comment clicks if in review mode
            if state.review_mode {
                let left_comments: Vec<&ReviewComment> = left_line
                    .and_then(|l| l.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Original && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let right_comments: Vec<&ReviewComment> = right_line
                    .and_then(|r| r.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Modified && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let max_comments = left_comments.len().max(right_comments.len());
                for c_idx in 0..max_comments {
                    if current_y + DIFF_COMMENT_HEIGHT > diff_y + diff_height {
                        break;
                    }

                    if y >= current_y && y < current_y + DIFF_COMMENT_HEIGHT {
                        // Left column comments
                        if x < diff_x + half_w
                            && let Some(c) = left_comments.get(c_idx)
                        {
                            let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                            let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                            if x >= card_x + card_w - 42.0 && x <= card_x + card_w - 24.0 {
                                return DiffAction::ToggleResolve(c.id);
                            }
                            if x >= card_x + card_w - 20.0 && x <= card_x + card_w - 4.0 {
                                return DiffAction::DeleteComment(c.id);
                            }
                        }
                        // Right column comments
                        if x >= diff_x + half_w
                            && let Some(c) = right_comments.get(c_idx)
                        {
                            let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                            let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                            if x >= card_x + card_w - 42.0 && x <= card_x + card_w - 24.0 {
                                return DiffAction::ToggleResolve(c.id);
                            }
                            if x >= card_x + card_w - 20.0 && x <= card_x + card_w - 4.0 {
                                return DiffAction::DeleteComment(c.id);
                            }
                        }
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }

                // Check clicks on in-progress draft comment card
                let has_left_draft = left_line.and_then(|l| l.line_number).is_some_and(|ln| {
                    state
                        .comment_draft
                        .as_ref()
                        .is_some_and(|d| d.side == DiffSide::Original && d.line_number == ln)
                });

                let has_right_draft = right_line.and_then(|r| r.line_number).is_some_and(|ln| {
                    state
                        .comment_draft
                        .as_ref()
                        .is_some_and(|d| d.side == DiffSide::Modified && d.line_number == ln)
                });

                if (has_left_draft || has_right_draft)
                    && current_y + DIFF_COMMENT_HEIGHT <= diff_y + diff_height
                {
                    if y >= current_y && y < current_y + DIFF_COMMENT_HEIGHT {
                        if has_left_draft
                            && x < diff_x + half_w
                            && let Some(draft) = &state.comment_draft
                        {
                            let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                            let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                            if x >= card_x + 6.0 && x <= card_x + 62.0 {
                                return DiffAction::SetDraftTag(draft.tag.cycle());
                            }
                            if x >= card_x + card_w - 64.0 && x <= card_x + card_w - 26.0 {
                                return DiffAction::SubmitDraft;
                            }
                            if x >= card_x + card_w - 24.0 && x <= card_x + card_w - 4.0 {
                                return DiffAction::CancelDraft;
                            }
                        }
                        if has_right_draft
                            && x >= diff_x + half_w
                            && let Some(draft) = &state.comment_draft
                        {
                            let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                            let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                            if x >= card_x + 6.0 && x <= card_x + 62.0 {
                                return DiffAction::SetDraftTag(draft.tag.cycle());
                            }
                            if x >= card_x + card_w - 64.0 && x <= card_x + card_w - 26.0 {
                                return DiffAction::SubmitDraft;
                            }
                            if x >= card_x + card_w - 24.0 && x <= card_x + card_w - 4.0 {
                                return DiffAction::CancelDraft;
                            }
                        }
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }
            }
        }
    }

    DiffAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_diff_view_state_has_hunk_and_comments() {
        let state = DiffViewState::default();
        assert_eq!(state.file_name, "sw.js");
        assert_eq!(state.hunks.len(), 1);
        assert!(state.review_mode);
        assert_eq!(state.review_verdict, ReviewVerdict::InReview);
        assert_eq!(state.comments.len(), 2);
    }

    #[test]
    fn comment_management_and_resolution() {
        let mut state = DiffViewState::default();
        let initial_count = state.comment_count();
        let new_id = state.add_comment(
            3,
            DiffSide::Modified,
            ReviewTag::Nit,
            "Check trailing comma".to_string(),
            "Akash".to_string(),
        );
        assert_eq!(state.comment_count(), initial_count + 1);

        let initial_resolved = state.resolved_comment_count();
        state.toggle_resolve(new_id);
        assert_eq!(state.resolved_comment_count(), initial_resolved + 1);

        state.delete_comment(new_id);
        assert_eq!(state.comment_count(), initial_count);
    }

    #[test]
    fn generate_review_markdown_includes_file_and_comments() {
        let state = DiffViewState::default();
        let md = state.generate_review_markdown();
        assert!(md.contains("# Code Review: sw.js"));
        assert!(md.contains("Consider pinning release"));
        assert!(md.contains("`SUGGEST`"));
        assert!(md.contains("[OPEN]"));
    }

    #[test]
    fn assemble_diff_view_quads_generates_quads() {
        let state = DiffViewState::default();
        let quads =
            assemble_diff_view_quads(200.0, 36.0, 600.0, 500.0, [0.0, 1.0, 0.5, 1.0], &state);
        assert_ne!(quads.len(), 0);
    }

    #[test]
    fn build_diff_view_labels_contains_code_and_buttons() {
        let state = DiffViewState::default();
        let labels = build_diff_view_labels(200.0, 36.0, 600.0, 500.0, &state);
        assert_ne!(labels.len(), 0);
        assert!(labels.iter().any(|l| l.text.contains("sw.js")));
        assert!(labels.iter().any(|l| l.text.contains("CACHE_NAME")));
        assert!(labels.iter().any(|l| l.text.contains("Appr")));
        assert!(labels.iter().any(|l| l.text.contains("Review")));
        assert!(labels.iter().any(|l| l.text.contains("AI")));
    }

    #[test]
    fn test_ai_review_engine_detects_issues_and_sets_verdict() {
        let mut state = DiffViewState::default();
        let count = state.run_ai_review();
        assert!(count > 0);
        assert!(state.review_mode);
        assert!(
            state.review_verdict == ReviewVerdict::Approved
                || state.review_verdict == ReviewVerdict::InReview
                || state.review_verdict == ReviewVerdict::ChangesRequested
        );
    }

    #[test]
    fn hit_test_diff_view_detects_buttons() {
        let state = DiffViewState::default();
        let x = 200.0;
        let y = 36.0;
        let w = 800.0;
        let h = 500.0;
        let layout = DiffHeaderLayout::compute(x, y, w, &state);

        assert_eq!(
            hit_test_diff_view(
                layout.review_tab_rect.0 + 10.0,
                y + 10.0,
                x,
                y,
                w,
                h,
                &state
            ),
            DiffAction::ToggleReviewMode
        );
        assert_eq!(
            hit_test_diff_view(
                layout.approve_btn_rect.unwrap().0 + 10.0,
                y + 10.0,
                x,
                y,
                w,
                h,
                &state
            ),
            DiffAction::SetVerdict(ReviewVerdict::Approved)
        );
        assert_eq!(
            hit_test_diff_view(
                layout.changes_btn_rect.unwrap().0 + 10.0,
                y + 10.0,
                x,
                y,
                w,
                h,
                &state
            ),
            DiffAction::SetVerdict(ReviewVerdict::ChangesRequested)
        );
        assert_eq!(
            hit_test_diff_view(layout.stage_btn_rect.0 + 10.0, y + 10.0, x, y, w, h, &state),
            DiffAction::ToggleStage
        );
        assert_eq!(
            hit_test_diff_view(layout.open_btn_rect.0 + 10.0, y + 10.0, x, y, w, h, &state),
            DiffAction::OpenFile
        );
        assert_eq!(
            hit_test_diff_view(layout.close_btn_rect.0 + 10.0, y + 10.0, x, y, w, h, &state),
            DiffAction::Close
        );
    }

    #[test]
    fn test_review_tag_cycle() {
        let tag = ReviewTag::Nit;
        assert_eq!(tag.cycle(), ReviewTag::Suggestion);
        assert_eq!(ReviewTag::Suggestion.cycle(), ReviewTag::Issue);
        assert_eq!(ReviewTag::Issue.cycle(), ReviewTag::Approval);
        assert_eq!(ReviewTag::Approval.cycle(), ReviewTag::Note);
        assert_eq!(ReviewTag::Note.cycle(), ReviewTag::Nit);
    }

    #[test]
    fn test_tokenize_syntax_line_languages() {
        // Rust
        let rust_spans = tokenize_syntax_line(
            "pub fn test() { let x = 42; }",
            "lib.rs",
            DiffLineType::Context,
        );
        assert!(rust_spans.iter().any(|s| s.text == "pub" && s.bold));
        assert!(rust_spans.iter().any(|s| s.text == "fn" && s.bold));
        assert!(rust_spans.iter().any(|s| s.text == "42"));

        // JavaScript
        let js_spans =
            tokenize_syntax_line("const name = 'oxidite';", "app.js", DiffLineType::Addition);
        assert!(js_spans.iter().any(|s| s.text == "const" && s.bold));
        assert!(js_spans.iter().any(|s| s.text == "'oxidite'"));

        // Python
        let py_spans =
            tokenize_syntax_line("def run(): # comment\n", "main.py", DiffLineType::Context);
        assert!(py_spans.iter().any(|s| s.text == "def" && s.bold));
        assert!(py_spans.iter().any(|s| s.text.contains("# comment")));
    }

    #[test]
    fn test_multi_file_navigation_layout_and_hit_test() {
        let mut state = DiffViewState::default();
        state.file_index = 1;
        state.total_files = 3;

        let x = 100.0;
        let y = 30.0;
        let w = 700.0;
        let h = 400.0;
        let layout = DiffHeaderLayout::compute(x, y, w, &state);

        assert!(layout.prev_file_rect.is_some());
        assert!(layout.file_counter_rect.is_some());
        assert!(layout.next_file_rect.is_some());

        let prev = layout.prev_file_rect.unwrap();
        let next = layout.next_file_rect.unwrap();

        assert_eq!(
            hit_test_diff_view(prev.0 + 5.0, prev.1 + 5.0, x, y, w, h, &state),
            DiffAction::PrevFile
        );
        assert_eq!(
            hit_test_diff_view(next.0 + 5.0, next.1 + 5.0, x, y, w, h, &state),
            DiffAction::NextFile
        );
    }

    #[test]
    fn test_comment_draft_lifecycle_and_hit_test() {
        let mut state = DiffViewState::default();
        state.comments.clear();
        state.comment_draft = Some(CommentDraft::new(1, DiffSide::Modified));

        let x = 0.0;
        let y = 0.0;
        let w = 800.0;
        let h = 600.0;

        let quads = assemble_diff_view_quads(x, y, w, h, [0.0, 1.0, 0.5, 1.0], &state);
        assert!(!quads.is_empty());

        let labels = build_diff_view_labels(x, y, w, h, &state);
        assert!(
            labels
                .iter()
                .any(|l| l.text.contains("Type review comment"))
        );

        // Tag button is at card_x + 6.0
        let half_w = (w / 2.0).floor();
        let card_x = x + half_w + DIFF_GUTTER_WIDTH + 3.0;
        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
        let draft_y = y + DIFF_HEADER_HEIGHT + DIFF_LINE_HEIGHT * 2.0;

        let tag_click = hit_test_diff_view(card_x + 10.0, draft_y + 10.0, x, y, w, h, &state);
        assert_eq!(tag_click, DiffAction::SetDraftTag(ReviewTag::Issue));

        let post_click =
            hit_test_diff_view(card_x + card_w - 40.0, draft_y + 10.0, x, y, w, h, &state);
        assert_eq!(post_click, DiffAction::SubmitDraft);

        let cancel_click =
            hit_test_diff_view(card_x + card_w - 10.0, draft_y + 10.0, x, y, w, h, &state);
        assert_eq!(cancel_click, DiffAction::CancelDraft);
    }
}
