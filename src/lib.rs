//! Extract plain text from Wikipedia article HTML.
//!
//! This crate parses Wikipedia article HTML using [tree-sitter](https://tree-sitter.github.io/)
//! and extracts clean, structured plain text — skipping navigation, infoboxes, references,
//! and other non-prose content.
//!
//! # Quick start
//!
//! ```rust
//! use wikipedia-article-transform::WikiPage;
//!
//! let html = r#"<html><body><p id="intro">Hello world.</p></body></html>"#;
//! let text = WikiPage::extract_text_plain(html).unwrap();
//! assert_eq!(text, "Hello world.");
//! ```
//!
//! For richer output with section tracking and tag context, use [`WikiPage::extract_text`]:
//!
//! ```rust
//! use wikipedia-article-transform::WikiPage;
//!
//! let html = r#"<html><body><h2>History</h2><p id="p1">Some text.</p></body></html>"#;
//! let mut page = WikiPage::new().unwrap();
//! let segments = page.extract_text(html).unwrap();
//! assert_eq!(segments[0].section, "History");
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
use std::{collections::HashMap, fmt};
use tree_sitter::{Node, Parser};
use tree_sitter_html::LANGUAGE;

/// A single paragraph-level text segment extracted from a Wikipedia article.
///
/// Each segment corresponds to a `<p>` block in the HTML (or any top-level text
/// node before the first paragraph). The segment captures its text content,
/// the full HTML tag ancestry, the MediaWiki paragraph ID, and the section path.
#[derive(Debug, Clone, Serialize)]
pub struct TextSegment {
    /// The extracted plain text of this segment.
    pub text: String,
    /// The chain of HTML tags from the document root down to this segment's container.
    pub tag_path: Vec<HtmlTag>,
    /// The `id` attribute of the enclosing `<p>` element, if present.
    pub mwid: String,
    /// The section heading path, e.g. `"History - Early life"`.
    pub section: String,
}

impl TextSegment {
    /// Returns `true` if any tag in the ancestry path has the given tag name.
    pub fn is_in_tag(&self, tag_name: &str) -> bool {
        self.tag_path.iter().any(|tag| tag.name == tag_name)
    }

    /// Returns `true` if any ancestor tag has the given attribute.
    ///
    /// If `attr_value` is `Some`, also checks that the attribute equals that value.
    pub fn is_in_tag_with_attribute(&self, attr_name: &str, attr_value: Option<&str>) -> bool {
        self.tag_path.iter().any(|tag| {
            if let Some(value) = attr_value {
                tag.get_attribute(attr_name).is_some_and(|v| v == value)
            } else {
                tag.has_attribute(attr_name)
            }
        })
    }

    /// Returns `true` if any ancestor tag has the given CSS class.
    pub fn is_in_tag_with_class(&self, class_name: &str) -> bool {
        self.tag_path.iter().any(|tag| tag.has_class(class_name))
    }

    /// Returns `true` if any ancestor tag has the given `id` attribute value.
    pub fn is_in_tag_with_id(&self, id: &str) -> bool {
        self.tag_path.iter().any(|tag| tag.has_id(id))
    }

    /// Returns the immediate parent tag of this segment, if any.
    pub fn get_parent_tag(&self) -> Option<&HtmlTag> {
        self.tag_path.last()
    }

    /// Returns `true` if the last N tags in the ancestry match the given CSS-like selectors.
    ///
    /// Selectors support `tag`, `.class`, and `#id` syntax. The selectors are matched
    /// against the tail of `tag_path`, so `["div", "p"]` matches any segment inside
    /// a `<p>` that is directly inside a `<div>`.
    pub fn matches_selector_path(&self, selectors: &[&str]) -> bool {
        if selectors.is_empty() || self.tag_path.len() < selectors.len() {
            return false;
        }
        let start_idx = self.tag_path.len() - selectors.len();
        selectors
            .iter()
            .enumerate()
            .all(|(i, sel)| self.tag_path[start_idx + i].matches_selector(sel))
    }
}

impl fmt::Display for TextSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mwid: {}\nsection: {}\ntext:\n{}\n---",
            self.mwid, self.section, self.text
        )
    }
}

/// An HTML element node in the tag ancestry path.
#[derive(Debug, Clone, Serialize)]
pub struct HtmlTag {
    /// The tag name, e.g. `"div"`, `"p"`, `"span"`.
    pub name: String,
    /// All HTML attributes on this element.
    pub attributes: HashMap<String, String>,
    /// Byte offset of the start of this element in the source HTML.
    pub start_byte: usize,
    /// Byte offset of the end of this element in the source HTML.
    pub end_byte: usize,
}

impl HtmlTag {
    /// Creates a new `HtmlTag` with no attributes.
    pub fn new(name: String, start_byte: usize, end_byte: usize) -> Self {
        Self {
            name,
            attributes: HashMap::new(),
            start_byte,
            end_byte,
        }
    }

    /// Returns `true` if this element has the given attribute (regardless of value).
    pub fn has_attribute(&self, name: &str) -> bool {
        self.attributes.contains_key(name)
    }

    /// Returns the value of the given attribute, or `None` if absent.
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(|s| s.as_str())
    }

    /// Returns `true` if the element's `class` attribute contains the given class name.
    pub fn has_class(&self, class_name: &str) -> bool {
        self.get_attribute("class")
            .is_some_and(|c| c.split_whitespace().any(|c| c == class_name))
    }

    /// Returns the `id` attribute value, or `None` if absent.
    pub fn get_id(&self) -> Option<&str> {
        self.get_attribute("id")
    }

    /// Returns `true` if this element's `id` attribute equals the given value.
    pub fn has_id(&self, id: &str) -> bool {
        self.get_attribute("id").is_some_and(|v| v == id)
    }

    /// Returns `true` if this element matches the given simple CSS selector.
    ///
    /// Supported selector forms:
    /// - `"div"` — matches by tag name
    /// - `".classname"` — matches by CSS class
    /// - `"#idvalue"` — matches by `id` attribute
    pub fn matches_selector(&self, selector: &str) -> bool {
        if selector.starts_with('.') {
            self.has_class(&selector[1..])
        } else if selector.starts_with('#') {
            self.has_id(&selector[1..])
        } else {
            self.name == selector
        }
    }
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
/// use wikipedia-article-transform::WikiPage;
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
        self.walk_and_collect(&tree.root_node(), Vec::new(), source);
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

    fn walk_and_collect(&mut self, node: &Node, mut current_tag_path: Vec<HtmlTag>, source: &[u8]) {
        match node.kind() {
            "text" => {
                if let Ok(text) = node.utf8_text(source) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        if self.text_segments.is_empty() {
                            self.text_segments.push(TextSegment {
                                text: String::new(),
                                tag_path: current_tag_path.clone(),
                                mwid: String::new(),
                                section: self.get_current_section_string(),
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
                if let Some(html_tag) = self.parse_element(node, source) {
                    if html_tag.name == "link" {
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
                    if EXCLUDED_CLASSES.iter().any(|c| html_tag.has_class(c)) {
                        return;
                    }

                    if let Some(level) = Self::get_header_level(&html_tag.name) {
                        let header_text = self.extract_text_from_element(node, source);
                        if !header_text.is_empty() {
                            self.update_sections(level, header_text);
                        }
                        // Do not recurse: header text is already captured above.
                        return;
                    } else if html_tag.name == "p" {
                        let mwid = html_tag.get_id().unwrap_or("").to_string();
                        let mut new_tag_path = current_tag_path.clone();
                        new_tag_path.push(html_tag);
                        self.text_segments.push(TextSegment {
                            text: String::new(),
                            tag_path: new_tag_path.clone(),
                            mwid,
                            section: self.get_current_section_string(),
                        });
                        current_tag_path = new_tag_path;
                    } else {
                        current_tag_path.push(html_tag);
                    }

                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i as u32) {
                            self.walk_and_collect(&child, current_tag_path.clone(), source);
                        }
                    }
                }
            }
            _ => {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i as u32) {
                        self.walk_and_collect(&child, current_tag_path.clone(), source);
                    }
                }
            }
        }
    }

    fn parse_element(&self, element_node: &Node, source: &[u8]) -> Option<HtmlTag> {
        let start_tag = element_node
            .children(&mut element_node.walk())
            .find(|child| child.kind() == "start_tag")?;

        let tag_name = start_tag
            .children(&mut start_tag.walk())
            .find(|child| child.kind() == "tag_name")?;

        let tag_name_str = tag_name.utf8_text(source).ok()?.to_string();
        let mut html_tag = HtmlTag::new(
            tag_name_str,
            element_node.start_byte(),
            element_node.end_byte(),
        );

        for child in start_tag.children(&mut start_tag.walk()) {
            if child.kind() == "attribute" {
                if let Some((name, value)) = self.parse_attribute(&child, source) {
                    html_tag.attributes.insert(name, value);
                }
            }
        }

        Some(html_tag)
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

/// Fetch a Wikipedia article by language code and title, returning extracted text segments.
///
/// Requires the `fetch` feature.
///
/// # Example
///
/// ```no_run
/// # #[cfg(feature = "fetch")]
/// # async fn example() -> anyhow::Result<()> {
/// let segments = wikipedia-article-transform::get_text("en", "Rust_(programming_language)").await?;
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
    fn test_is_in_tag() {
        let segs = extract(r#"<div class="content"><p>Text.</p></div>"#);
        assert!(!segs.is_empty());
        assert!(segs[0].is_in_tag("div"));
        assert!(!segs[0].is_in_tag("span"));
    }

    #[test]
    fn test_is_in_tag_with_class() {
        let segs = extract(r#"<div class="content mw-body"><p>Text.</p></div>"#);
        assert!(!segs.is_empty());
        assert!(segs[0].is_in_tag_with_class("mw-body"));
        assert!(!segs[0].is_in_tag_with_class("sidebar"));
    }

    #[test]
    fn test_matches_selector_path() {
        let segs = extract(r#"<div id="main"><p id="p1">Text.</p></div>"#);
        assert!(!segs.is_empty());
        assert!(segs[0].matches_selector_path(&["div", "p"]));
        assert!(!segs[0].matches_selector_path(&["section", "p"]));
    }

    #[test]
    fn test_html_tag_selector() {
        let tag = HtmlTag {
            name: "div".into(),
            attributes: [
                ("class".into(), "foo bar".into()),
                ("id".into(), "main".into()),
            ]
            .into(),
            start_byte: 0,
            end_byte: 10,
        };
        assert!(tag.matches_selector("div"));
        assert!(tag.matches_selector(".foo"));
        assert!(tag.matches_selector(".bar"));
        assert!(tag.matches_selector("#main"));
        assert!(!tag.matches_selector(".baz"));
        assert!(!tag.matches_selector("#other"));
        assert!(!tag.matches_selector("span"));
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
    fn test_get_id() {
        let tag = HtmlTag::new("p".into(), 0, 5);
        assert_eq!(tag.get_id(), None);

        let mut tag2 = HtmlTag::new("p".into(), 0, 5);
        tag2.attributes.insert("id".into(), "mw42".into());
        assert_eq!(tag2.get_id(), Some("mw42"));
    }
}
