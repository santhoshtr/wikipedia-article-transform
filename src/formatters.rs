//! Output formatters for Wikipedia article items.
//!
//! Provides the [`ArticleFormat`] trait with three output formats:
//! - [`ArticleFormat::format_plain`] — plain text with heading lines and image placeholders
//! - [`ArticleFormat::format_json`] — semantic JSON section tree with images and references
//! - [`ArticleFormat::format_markdown`] — Markdown with inline formatting, images, and footnotes

use std::collections::HashMap;

use crate::{ArticleItem, ImageSegment, InlineNode};
use serde::Serialize;

/// Output formatting for a collection of [`ArticleItem`]s.
pub trait ArticleFormat {
    /// Format as plain text.
    ///
    /// Section headings are emitted as `#`/`##`/`###` lines. Images are rendered
    /// as `[Image: alt text]` followed by caption. References are omitted.
    fn format_plain(&self) -> String;

    /// Format as a semantic JSON section tree.
    ///
    /// Structure:
    /// ```json
    /// {
    ///   "intro": ["..."], "intro_images": [...],
    ///   "sections": [{"heading":"...","level":2,"paragraphs":[...],"images":[...],"subsections":[...]}],
    ///   "references": {"cite_note-Foo-1": "Full citation text..."}
    /// }
    /// ```
    fn format_json(&self) -> anyhow::Result<String>;

    /// Format as Markdown.
    ///
    /// Inline: bold → `**text**`, italic → `_text_`, links → `[text](href)`,
    /// citation refs → `[N]`. Images → `![alt](src)` with italic caption.
    /// A `## References` section with `[N]: citation` definitions is appended
    /// when references are present.
    fn format_markdown(&self) -> String;
}

impl ArticleFormat for Vec<ArticleItem> {
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

impl ArticleFormat for &[ArticleItem] {
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

fn emit_section_heading(
    out: &mut String,
    seg_section: &str,
    seg_level: u8,
    last_section: &mut String,
) {
    if seg_section != *last_section {
        if !out.is_empty() {
            out.push('\n');
        }
        if !seg_section.is_empty() {
            let hashes = "#".repeat(seg_level.max(1) as usize);
            let heading = seg_section.rsplit(" - ").next().unwrap_or(seg_section);
            out.push_str(&hashes);
            out.push(' ');
            out.push_str(heading);
            out.push('\n');
        }
        *last_section = seg_section.to_string();
    }
}

fn format_plain(items: &[ArticleItem]) -> String {
    let mut out = String::new();
    let mut last_section = String::new();

    for item in items {
        match item {
            ArticleItem::Paragraph(seg) => {
                let text = seg.text.trim();
                if text.is_empty() {
                    continue;
                }
                emit_section_heading(&mut out, &seg.section, seg.section_level, &mut last_section);
                out.push('\n');
                out.push_str(text);
                out.push('\n');
            }
            ArticleItem::Image(img) => {
                emit_section_heading(&mut out, &img.section, img.section_level, &mut last_section);
                out.push('\n');
                out.push_str("[Image: ");
                out.push_str(&img.alt);
                out.push(']');
                out.push('\n');
                if !img.caption.is_empty() {
                    out.push_str(&img.caption);
                    out.push('\n');
                }
            }
            ArticleItem::References(_) => {} // omit from plain output
        }
    }

    out
}

fn format_json(items: &[ArticleItem]) -> anyhow::Result<String> {
    #[derive(Serialize)]
    struct ImageEntry {
        src: String,
        alt: String,
        caption: String,
    }

    impl From<&ImageSegment> for ImageEntry {
        fn from(img: &ImageSegment) -> Self {
            ImageEntry {
                src: img.src.clone(),
                alt: img.alt.clone(),
                caption: img.caption.clone(),
            }
        }
    }

    #[derive(Serialize)]
    struct Section {
        heading: String,
        level: u8,
        paragraphs: Vec<String>,
        images: Vec<ImageEntry>,
        subsections: Vec<Section>,
    }

    #[derive(Serialize)]
    struct ArticleTree {
        intro: Vec<String>,
        intro_images: Vec<ImageEntry>,
        sections: Vec<Section>,
        references: HashMap<String, String>,
    }

    let mut tree = ArticleTree {
        intro: Vec::new(),
        intro_images: Vec::new(),
        sections: Vec::new(),
        references: HashMap::new(),
    };

    for item in items {
        match item {
            ArticleItem::Paragraph(seg) => {
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
                    let depth_from_bottom = (parts.len() - 1 - i) as u8;
                    let level = seg.section_level.saturating_sub(depth_from_bottom);
                    if !siblings.iter().any(|s: &Section| s.heading == *part) {
                        siblings.push(Section {
                            heading: part.to_string(),
                            level,
                            paragraphs: Vec::new(),
                            images: Vec::new(),
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
            ArticleItem::Image(img) => {
                let entry = ImageEntry::from(img);
                if img.section.is_empty() {
                    tree.intro_images.push(entry);
                    continue;
                }
                let parts: Vec<&str> = img.section.split(" - ").collect();
                let mut siblings = &mut tree.sections;
                for (i, part) in parts.iter().enumerate() {
                    let depth_from_bottom = (parts.len() - 1 - i) as u8;
                    let level = img.section_level.saturating_sub(depth_from_bottom);
                    if !siblings.iter().any(|s: &Section| s.heading == *part) {
                        siblings.push(Section {
                            heading: part.to_string(),
                            level,
                            paragraphs: Vec::new(),
                            images: Vec::new(),
                            subsections: Vec::new(),
                        });
                    }
                    let idx = siblings.iter().position(|s| s.heading == *part).unwrap();
                    if i == parts.len() - 1 {
                        siblings[idx].images.push(entry);
                        break;
                    } else {
                        siblings = &mut siblings[idx].subsections;
                    }
                }
            }
            ArticleItem::References(refs) => {
                tree.references = refs.clone();
            }
        }
    }

    Ok(serde_json::to_string_pretty(&tree)?)
}

/// Sort a reference map by the trailing integer in the note_id (`cite_note-Name-N`).
fn sorted_refs(refs: &HashMap<String, String>) -> Vec<(&String, &String)> {
    let mut entries: Vec<(&String, &String)> = refs.iter().collect();
    entries.sort_by_key(|(note_id, _)| {
        note_id
            .rsplit('-')
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    entries
}

fn format_markdown(items: &[ArticleItem]) -> String {
    let mut out = String::new();
    let mut last_section = String::new();

    for item in items {
        match item {
            ArticleItem::Paragraph(seg) => {
                if seg.text.trim().is_empty() {
                    continue;
                }
                emit_section_heading(&mut out, &seg.section, seg.section_level, &mut last_section);
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
                        InlineNode::Ref { label, .. } => {
                            // Append [N] directly — no space before a superscript marker
                            para.push('[');
                            para.push('^');
                            para.push_str(label);
                            para.push(']');
                        }
                    }
                }
                out.push_str(para.trim_end());
                out.push('\n');
            }
            ArticleItem::Image(img) => {
                emit_section_heading(&mut out, &img.section, img.section_level, &mut last_section);
                out.push('\n');
                out.push_str("![");
                out.push_str(&img.alt);
                out.push_str("](");
                out.push_str(&img.src);
                out.push(')');
                out.push('\n');
                if !img.caption.is_empty() {
                    out.push('_');
                    out.push_str(&img.caption);
                    out.push('_');
                    out.push('\n');
                }
            }
            ArticleItem::References(refs) => {
                if refs.is_empty() {
                    continue;
                }
                out.push_str("\n## References\n");
                for (note_id, citation) in sorted_refs(refs) {
                    // Extract the numeric label from the note_id tail
                    let label = note_id.rsplit('-').next().unwrap_or(note_id.as_str());
                    out.push('\n');
                    out.push('[');
                    out.push('^');
                    out.push_str(label);
                    out.push_str("]: ");
                    out.push_str(citation);
                    out.push('\n');
                }
            }
        }
    }

    out
}
