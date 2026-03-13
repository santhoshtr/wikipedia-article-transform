//! Output formatters for Wikipedia article text segments.
//!
//! Provides the [`ArticleFormat`] trait with three output formats:
//! - [`ArticleFormat::format_plain`] — plain text with heading lines
//! - [`ArticleFormat::format_json`] — semantic JSON section tree
//! - [`ArticleFormat::format_markdown`] — Markdown with inline formatting

use crate::{InlineNode, TextSegment};
use serde::Serialize;

/// Output formatting for a collection of [`TextSegment`]s.
pub trait ArticleFormat {
    /// Format as plain text.
    ///
    /// Section headings are emitted as `#`/`##`/`###` lines matching heading depth,
    /// only when the section changes. Paragraphs are separated by blank lines.
    /// No metadata labels or separators.
    fn format_plain(&self) -> String;

    /// Format as a semantic JSON section tree.
    ///
    /// Structure:
    /// ```json
    /// {
    ///   "intro": ["paragraphs before first heading"],
    ///   "sections": [
    ///     { "heading": "...", "level": 2, "paragraphs": ["..."], "subsections": [...] }
    ///   ]
    /// }
    /// ```
    fn format_json(&self) -> anyhow::Result<String>;

    /// Format as Markdown.
    ///
    /// Section headings become `##`/`###` etc. Inline content is rendered:
    /// bold → `**text**`, italic → `_text_`, links → `[text](href)`.
    fn format_markdown(&self) -> String;
}

impl ArticleFormat for Vec<TextSegment> {
    fn format_plain(&self) -> String {
        format_plain(self)
    }

    fn format_json(&self) -> anyhow::Result<String> {
        format_json(self)
    }

    fn format_markdown(&self) -> String {
        format_markdown(self)
    }
}

impl ArticleFormat for &[TextSegment] {
    fn format_plain(&self) -> String {
        format_plain(self)
    }

    fn format_json(&self) -> anyhow::Result<String> {
        format_json(self)
    }

    fn format_markdown(&self) -> String {
        format_markdown(self)
    }
}

fn format_plain(segments: &[TextSegment]) -> String {
    let mut out = String::new();
    let mut last_section = String::new();

    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }

        if seg.section != last_section {
            if !out.is_empty() {
                out.push('\n');
            }
            if !seg.section.is_empty() {
                let hashes = "#".repeat(seg.section_level.max(1) as usize);
                let heading = seg.section.rsplit(" - ").next().unwrap_or(&seg.section);
                out.push_str(&hashes);
                out.push(' ');
                out.push_str(heading);
                out.push('\n');
            }
            last_section = seg.section.clone();
        }

        out.push('\n');
        out.push_str(text);
        out.push('\n');
    }

    out
}

fn format_json(segments: &[TextSegment]) -> anyhow::Result<String> {
    #[derive(Serialize)]
    struct Section {
        heading: String,
        level: u8,
        paragraphs: Vec<String>,
        subsections: Vec<Section>,
    }

    #[derive(Serialize)]
    struct ArticleTree {
        intro: Vec<String>,
        sections: Vec<Section>,
    }

    let mut tree = ArticleTree {
        intro: Vec::new(),
        sections: Vec::new(),
    };

    for seg in segments {
        let text = seg.text.trim().to_string();
        if text.is_empty() {
            continue;
        }

        if seg.section.is_empty() {
            tree.intro.push(text);
            continue;
        }

        let parts: Vec<&str> = seg.section.split(" - ").collect();
        let mut siblings = &mut tree.sections;

        for (i, part) in parts.iter().enumerate() {
            // Compute level for this path component.
            // section_level is the deepest level; back-calculate ancestors.
            let depth_from_bottom = (parts.len() - 1 - i) as u8;
            let level = seg.section_level.saturating_sub(depth_from_bottom);

            if !siblings.iter().any(|s: &Section| s.heading == *part) {
                siblings.push(Section {
                    heading: part.to_string(),
                    level,
                    paragraphs: Vec::new(),
                    subsections: Vec::new(),
                });
            }
            let idx = siblings.iter().position(|s| s.heading == *part).unwrap();

            if i == parts.len() - 1 {
                siblings[idx].paragraphs.push(text.clone());
                break;
            } else {
                siblings = &mut siblings[idx].subsections;
            }
        }
    }

    Ok(serde_json::to_string_pretty(&tree)?)
}

fn format_markdown(segments: &[TextSegment]) -> String {
    let mut out = String::new();
    let mut last_section = String::new();

    for seg in segments {
        if seg.text.trim().is_empty() {
            continue;
        }

        if seg.section != last_section {
            if !out.is_empty() {
                out.push('\n');
            }
            if !seg.section.is_empty() {
                let hashes = "#".repeat(seg.section_level.max(1) as usize);
                let heading = seg.section.rsplit(" - ").next().unwrap_or(&seg.section);
                out.push_str(&hashes);
                out.push(' ');
                out.push_str(heading);
                out.push('\n');
            }
            last_section = seg.section.clone();
        }

        out.push('\n');
        let mut para = String::new();
        for node in &seg.content {
            match node {
                InlineNode::Text(s) => para.push_str(s),
                InlineNode::Bold(s) => {
                    if !para.ends_with(' ') && !para.is_empty() {
                        para.push(' ');
                    }
                    para.push_str("**");
                    para.push_str(s);
                    para.push_str("** ");
                }
                InlineNode::Italic(s) => {
                    if !para.ends_with(' ') && !para.is_empty() {
                        para.push(' ');
                    }
                    para.push('_');
                    para.push_str(s);
                    para.push_str("_ ");
                }
                InlineNode::Link { text, href } => {
                    if !para.ends_with(' ') && !para.is_empty() {
                        para.push(' ');
                    }
                    para.push('[');
                    para.push_str(text);
                    para.push_str("](");
                    para.push_str(href);
                    para.push_str(") ");
                }
            }
        }
        out.push_str(para.trim_end());
        out.push('\n');
    }

    out
}
