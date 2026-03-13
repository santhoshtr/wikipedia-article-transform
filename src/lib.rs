//! Extract plain text from Wikipedia article HTML.
//!
//! This crate parses Wikipedia article HTML using [tree-sitter](https://tree-sitter.github.io/)
//! and extracts clean, structured plain text — skipping navigation, infoboxes, references,
//! and other non-prose content.
//!
//! # Quick start
//!
//! ```rust
//! use wikipedia_article_transform::WikiPage;
//!
//! let html = r#"<html><body><p id="intro">Hello world.</p></body></html>"#;
//! let text = WikiPage::extract_text_plain(html).unwrap();
//! assert_eq!(text, "Hello world.");
//! ```
//!
//! For richer output with section tracking, use [`WikiPage::extract_text`]:
//!
//! ```rust
//! use wikipedia_article_transform::WikiPage;
//!
//! let html = r#"<html><body><h2>History</h2><p id="p1">Some text.</p></body></html>"#;
//! let mut page = WikiPage::new().unwrap();
//! let segments = page.extract_text(html).unwrap();
//! assert_eq!(segments[0].section, "History");
//! assert_eq!(segments[0].section_level, 2);
//! assert_eq!(segments[0].text, "Some text.");
//! ```
//!
//! # Optional feature: `fetch`
//!
//! Enable the `fetch` feature to fetch Wikipedia articles directly via the REST API:
//!
//! ```toml
//! wikipedia-article-transform = { version = "0.1", features = ["fetch"] }
//! ```

use serde::Serialize;
use tree_sitter::{Node, Parser};
use tree_sitter_html::LANGUAGE;

/// A single paragraph-level text segment extracted from a Wikipedia article.
///
/// Each segment corresponds to a `<p>` block in the HTML. It captures the paragraph
/// text, the MediaWiki paragraph ID, the section heading path, and the heading depth.
#[derive(Debug, Clone, Serialize)]
pub struct TextSegment {
    /// The extracted plain text of this segment.
    pub text: String,
    /// The `id` attribute of the enclosing `<p>` element, if present.
    pub mwid: String,
    /// The section heading path, e.g. `"History - Early life"`.
    pub section: String,
    /// The heading level of the current section (1–6). 0 if before any heading.
    pub section_level: u8,
}

#[derive(Debug, Clone)]
struct SectionInfo {
    title: String,
    level: u8,
}

/// A reusable Wikipedia HTML parser.
///
/// Reusing a single `WikiPage` instance across multiple articles is more efficient
/// than creating one per article, since it avoids re-initialising the tree-sitter
/// parser and grammar on each call.
///
/// # Example
///
/// ```rust
/// use wikipedia_article_transform::WikiPage;
///
/// let mut page = WikiPage::new().unwrap();
/// let segments = page.extract_text("<p>Hello.</p>").unwrap();
/// assert_eq!(segments[0].text, "Hello.");
/// ```
pub struct WikiPage {
    parser: Parser,
    text_segments: Vec<TextSegment>,
    current_sections: Vec<SectionInfo>,
}

impl WikiPage {
    /// Creates a new `WikiPage`, initialising the tree-sitter HTML parser.
    ///
    /// Returns an error if the HTML grammar cannot be loaded, which should not
    /// happen in practice since the grammar is statically compiled in.
    pub fn new() -> anyhow::Result<Self> {
        let language = LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        Ok(WikiPage {
            parser,
            text_segments: Vec::new(),
            current_sections: Vec::new(),
        })
    }

    /// Parses `html` and returns one [`TextSegment`] per paragraph.
    ///
    /// The parser state is reset on each call, so the same `WikiPage` can be
    /// reused safely across multiple articles.
    ///
    /// Skipped elements: `<script>`, `<style>`, `<link>`, and elements with
    /// classes `shortdescription`, `hatnote`, `infobox`, `reference`, `navbox`,
    /// `noprint`, `reflist`, `citation`.
    pub fn extract_text(&mut self, html: &str) -> anyhow::Result<Vec<TextSegment>> {
        self.text_segments.clear();
        self.current_sections.clear();
        let tree = self
            .parser
            .parse(html, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse HTML"))?;
        let source = html.as_bytes();
        self.walk_and_collect(&tree.root_node(), source);
        Ok(self.text_segments.clone())
    }

    /// Convenience method: parse `html` and return all paragraph text joined by `"\n\n"`.
    ///
    /// Creates a temporary `WikiPage` internally. Use [`WikiPage::extract_text`] directly
    /// when processing many articles to avoid re-initialising the parser.
    pub fn extract_text_plain(html: &str) -> anyhow::Result<String> {
        let mut page = WikiPage::new()?;
        let segments = page.extract_text(html)?;
        let text = segments
            .iter()
            .map(|s| s.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(text)
    }

    fn get_header_level(tag_name: &str) -> Option<u8> {
        match tag_name {
            "h1" => Some(1),
            "h2" => Some(2),
            "h3" => Some(3),
            "h4" => Some(4),
            "h5" => Some(5),
            "h6" => Some(6),
            _ => None,
        }
    }

    fn extract_text_from_element(&self, node: &Node, source: &[u8]) -> String {
        let mut text = String::new();
        for child in node.children(&mut node.walk()) {
            match child.kind() {
                "text" => {
                    if let Ok(t) = child.utf8_text(source) {
                        text.push_str(t.trim());
                    }
                }
                "element" => {
                    let child_text = self.extract_text_from_element(&child, source);
                    if !child_text.is_empty() {
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(&child_text);
                    }
                }
                _ => {}
            }
        }
        text
    }

    fn update_sections(&mut self, level: u8, title: String) {
        self.current_sections
            .retain(|section| section.level < level);
        self.current_sections.push(SectionInfo { title, level });
    }

    fn get_current_section_string(&self) -> String {
        self.current_sections
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>()
            .join(" - ")
    }

    fn get_current_section_level(&self) -> u8 {
        self.current_sections
            .last()
            .map(|s| s.level)
            .unwrap_or(0)
    }

    fn walk_and_collect(&mut self, node: &Node, source: &[u8]) {
        match node.kind() {
            "text" => {
                if let Ok(text) = node.utf8_text(source) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        if self.text_segments.is_empty() {
                            self.text_segments.push(TextSegment {
                                text: String::new(),
                                mwid: String::new(),
                                section: self.get_current_section_string(),
                                section_level: self.get_current_section_level(),
                            });
                        }
                        let last = self.text_segments.len() - 1;
                        if !self.text_segments[last].text.is_empty() {
                            self.text_segments[last].text.push(' ');
                        }
                        self.text_segments[last].text.push_str(trimmed);
                    }
                }
            }
            "script_element" | "style_element" => (),
            "element" => {
                if let Some((tag_name, attributes)) = self.parse_element(node, source) {
                    if tag_name == "link" {
                        return;
                    }

                    const EXCLUDED_CLASSES: &[&str] = &[
                        "shortdescription",
                        "hatnote",
                        "infobox",
                        "reference",
                        "navbox",
                        "noprint",
                        "reflist",
                        "citation",
                    ];
                    let class_attr = attributes.iter()
                        .find(|(k, _)| k == "class")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("");
                    if EXCLUDED_CLASSES.iter().any(|c| {
                        class_attr.split_whitespace().any(|cls| cls == *c)
                    }) {
                        return;
                    }

                    if let Some(level) = Self::get_header_level(&tag_name) {
                        let header_text = self.extract_text_from_element(node, source);
                        if !header_text.is_empty() {
                            self.update_sections(level, header_text);
                        }
                        return;
                    } else if tag_name == "p" {
                        let mwid = attributes.iter()
                            .find(|(k, _)| k == "id")
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default();
                        self.text_segments.push(TextSegment {
                            text: String::new(),
                            mwid,
                            section: self.get_current_section_string(),
                            section_level: self.get_current_section_level(),
                        });
                    }

                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i as u32) {
                            self.walk_and_collect(&child, source);
                        }
                    }
                }
            }
            _ => {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i as u32) {
                        self.walk_and_collect(&child, source);
                    }
                }
            }
        }
    }

    /// Returns `(tag_name, attributes)` for an element node, or `None` if unparseable.
    fn parse_element(&self, element_node: &Node, source: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        let start_tag = element_node
            .children(&mut element_node.walk())
            .find(|child| child.kind() == "start_tag")?;

        let tag_name_node = start_tag
            .children(&mut start_tag.walk())
            .find(|child| child.kind() == "tag_name")?;

        let tag_name = tag_name_node.utf8_text(source).ok()?.to_string();
        let mut attributes = Vec::new();

        for child in start_tag.children(&mut start_tag.walk()) {
            if child.kind() == "attribute" {
                if let Some(pair) = self.parse_attribute(&child, source) {
                    attributes.push(pair);
                }
            }
        }

        Some((tag_name, attributes))
    }

    fn parse_attribute(&self, attr_node: &Node, source: &[u8]) -> Option<(String, String)> {
        let mut attr_name = None;
        let mut attr_value = String::new();

        for child in attr_node.children(&mut attr_node.walk()) {
            match child.kind() {
                "attribute_name" => {
                    attr_name = child.utf8_text(source).ok().map(|s| s.to_string());
                }
                "quoted_attribute_value" => {
                    for grandchild in child.children(&mut child.walk()) {
                        if grandchild.kind() == "attribute_value" {
                            if let Ok(value) = grandchild.utf8_text(source) {
                                attr_value = value.to_string();
                            }
                        }
                    }
                }
                "attribute_value" => {
                    if let Ok(value) = child.utf8_text(source) {
                        attr_value = value.to_string();
                    }
                }
                _ => {}
            }
        }

        attr_name.map(|name| (name, attr_value))
    }
}

impl Default for WikiPage {
    fn default() -> Self {
        Self::new().expect("Failed to initialise tree-sitter HTML parser")
    }
}

/// Format a slice of [`TextSegment`]s as plain text.
///
/// Section headings are emitted as `#`/`##`/`###` lines (matching heading depth) when
/// the section changes. Paragraphs are separated by blank lines. No metadata labels.
///
/// # Example
///
/// ```rust
/// use wikipedia_article_transform::{WikiPage, format_plain};
///
/// let html = "<h2>History</h2><p>Para one.</p><h3>Early life</h3><p>Para two.</p>";
/// let mut page = WikiPage::new().unwrap();
/// let segments = page.extract_text(html).unwrap();
/// let text = format_plain(&segments);
/// assert!(text.contains("## History\n"));
/// assert!(text.contains("### Early life\n"));
/// ```
pub fn format_plain(segments: &[TextSegment]) -> String {
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
                // Emit only the deepest heading component
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

/// Format a slice of [`TextSegment`]s as a semantic JSON section tree.
///
/// The output groups paragraphs by section hierarchy:
///
/// ```json
/// {
///   "intro": ["Paragraphs before first heading..."],
///   "sections": [
///     {
///       "heading": "History",
///       "level": 2,
///       "paragraphs": ["..."],
///       "subsections": [
///         { "heading": "Early life", "level": 3, "paragraphs": ["..."], "subsections": [] }
///       ]
///     }
///   ]
/// }
/// ```
///
/// `mwid` is omitted — it is an internal MediaWiki detail.
pub fn format_json(segments: &[TextSegment]) -> anyhow::Result<String> {
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

        // Walk the heading path components to find/create the right node.
        let parts: Vec<&str> = seg.section.split(" - ").collect();
        let mut siblings = &mut tree.sections;

        for (i, part) in parts.iter().enumerate() {
            let level = if i == 0 {
                // Top-level: use section_level adjusted back to root depth.
                // section_level is the deepest heading; parts.len() tells us depth.
                seg.section_level.saturating_sub((parts.len() - 1) as u8)
            } else {
                seg.section_level.saturating_sub((parts.len() - 1 - i) as u8)
            };

            let pos = siblings.iter().position(|s| s.heading == *part);
            if pos.is_none() {
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
            } else {
                siblings = &mut siblings[idx].subsections;
            }
        }
    }

    Ok(serde_json::to_string_pretty(&tree)?)
}

/// Fetch a Wikipedia article by language code and title, returning extracted text segments.
///
/// Requires the `fetch` feature.
///
/// # Example
///
/// ```no_run
/// # #[cfg(feature = "fetch")]
/// # async fn example() -> anyhow::Result<()> {
/// let segments = wikipedia_article_transform::get_text("en", "Rust_(programming_language)").await?;
/// for seg in &segments {
///     println!("{}", seg.text);
/// }
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "fetch")]
pub async fn get_text(language: &str, title: &str) -> anyhow::Result<Vec<TextSegment>> {
    let html = get_page_content_html(language, title).await?;
    let mut page = WikiPage::new()?;
    Ok(page.extract_text(&html)?)
}

#[cfg(feature = "fetch")]
async fn get_page_content_html(language: &str, title: &str) -> anyhow::Result<String> {
    let url = format!("https://{language}.wikipedia.org/api/rest_v1/page/html/{title}?stash=false");
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "wikipedia-article-transform/0.1 (https://github.com/smc/wikisentences)",
        )
        .send()
        .await?;
    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch article: HTTP {}", response.status());
    }
    Ok(response.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(html: &str) -> Vec<TextSegment> {
        WikiPage::extract_text_plain(html).unwrap();
        let mut page = WikiPage::new().unwrap();
        page.extract_text(html).unwrap()
    }

    #[test]
    fn test_basic_paragraph() {
        let segs = extract("<html><body><p id=\"p1\">Hello world.</p></body></html>");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Hello world.");
        assert_eq!(segs[0].mwid, "p1");
        assert_eq!(segs[0].section, "");
        assert_eq!(segs[0].section_level, 0);
    }

    #[test]
    fn test_multiple_paragraphs() {
        let segs = extract("<p>First.</p><p>Second.</p><p>Third.</p>");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "First.");
        assert_eq!(segs[1].text, "Second.");
        assert_eq!(segs[2].text, "Third.");
    }

    #[test]
    fn test_section_tracking() {
        let html = "<h2>History</h2><p>Para one.</p><h3>Early life</h3><p>Para two.</p>";
        let segs = extract(html);
        assert_eq!(segs[0].section, "History");
        assert_eq!(segs[1].section, "History - Early life");
    }

    #[test]
    fn test_section_level() {
        let html = "<h2>History</h2><p>A.</p><h3>Early life</h3><p>B.</p>";
        let segs = extract(html);
        assert_eq!(segs[0].section_level, 2);
        assert_eq!(segs[1].section_level, 3);
    }

    #[test]
    fn test_section_resets_at_same_level() {
        let html = "<h2>History</h2><p>A.</p><h2>Geography</h2><p>B.</p>";
        let segs = extract(html);
        assert_eq!(segs[0].section, "History");
        assert_eq!(segs[1].section, "Geography");
    }

    #[test]
    fn test_excluded_class_infobox() {
        let html = r#"<p>Visible.</p><table class="infobox"><tr><td>Hidden.</td></tr></table><p>Also visible.</p>"#;
        let segs = extract(html);
        assert!(segs.iter().all(|s| !s.text.contains("Hidden")));
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn test_excluded_class_reflist() {
        let html = r#"<p>Main text.</p><div class="reflist"><p>Ref text.</p></div>"#;
        let segs = extract(html);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Main text.");
    }

    #[test]
    fn test_script_and_style_skipped() {
        let html = "<p>Real.</p><script>var x=1;</script><style>body{}</style><p>Also real.</p>";
        let segs = extract(html);
        assert_eq!(segs.len(), 2);
        assert!(segs.iter().all(|s| !s.text.contains("var x")));
    }

    #[test]
    fn test_empty_html() {
        let segs = extract("");
        assert!(segs.is_empty());
    }

    #[test]
    fn test_extract_text_plain() {
        let html = "<p>First paragraph.</p><p>Second paragraph.</p>";
        let text = WikiPage::extract_text_plain(html).unwrap();
        assert_eq!(text, "First paragraph.\n\nSecond paragraph.");
    }

    #[test]
    fn test_default_impl() {
        let mut page = WikiPage::default();
        let segs = page.extract_text("<p>Works.</p>").unwrap();
        assert_eq!(segs[0].text, "Works.");
    }

    #[test]
    fn test_format_plain_sections() {
        let html = "<p>Intro.</p><h2>History</h2><p>A.</p><h3>Early life</h3><p>B.</p>";
        let segs = extract(html);
        let out = format_plain(&segs);
        assert!(out.contains("\nIntro.\n"), "intro paragraph missing");
        assert!(out.contains("## History\n"), "h2 heading missing");
        assert!(out.contains("\nA.\n"), "first section paragraph missing");
        assert!(out.contains("### Early life\n"), "h3 heading missing");
        assert!(out.contains("\nB.\n"), "subsection paragraph missing");
        // headings appear before their paragraphs
        assert!(out.find("## History").unwrap() < out.find("\nA.\n").unwrap());
        assert!(out.find("### Early life").unwrap() < out.find("\nB.\n").unwrap());
    }

    #[test]
    fn test_format_json_tree() {
        let html = "<p>Intro.</p><h2>History</h2><p>A.</p><h3>Early life</h3><p>B.</p>";
        let segs = extract(html);
        let json_str = format_json(&segs).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["intro"][0], "Intro.");
        assert_eq!(v["sections"][0]["heading"], "History");
        assert_eq!(v["sections"][0]["level"], 2);
        assert_eq!(v["sections"][0]["paragraphs"][0], "A.");
        assert_eq!(v["sections"][0]["subsections"][0]["heading"], "Early life");
        assert_eq!(v["sections"][0]["subsections"][0]["level"], 3);
        assert_eq!(v["sections"][0]["subsections"][0]["paragraphs"][0], "B.");
    }
}
