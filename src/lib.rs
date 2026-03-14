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
//! For richer output with section tracking and inline structure, use [`WikiPage::extract_text`]:
//!
//! ```rust
//! use wikipedia_article_transform::{WikiPage, ArticleItem};
//!
//! let html = r#"<html><body><h2>History</h2><p id="p1">Some text.</p></body></html>"#;
//! let mut page = WikiPage::new().unwrap();
//! let items = page.extract_text(html).unwrap();
//! if let ArticleItem::Paragraph(seg) = &items[0] {
//!     assert_eq!(seg.section, "History");
//!     assert_eq!(seg.section_level, 2);
//!     assert_eq!(seg.text, "Some text.");
//! }
//! ```
//!
//! # Optional feature: `fetch`
//!
//! Enable the `fetch` feature to fetch Wikipedia articles directly via the REST API:
//!
//! ```toml
//! wikipedia-article-transform = { version = "0.1", features = ["cli"] }
//! ```

pub mod formatters;
pub use formatters::ArticleFormat;

use std::collections::HashMap;

use serde::Serialize;
use tree_sitter::{Node, Parser};
use tree_sitter_html::LANGUAGE;

/// An inline content node within a paragraph.
///
/// Captures the inline structure of paragraph text so formatters can render
/// bold, italic, link, and citation reference markup.
#[derive(Debug, Clone)]
pub enum InlineNode {
    /// Plain text.
    Text(String),
    /// Bold text (`<b>` or `<strong>`).
    Bold(String),
    /// Italic text (`<i>` or `<em>`).
    Italic(String),
    /// A hyperlink (`<a href="...">`).
    Link { text: String, href: String },
    /// A citation reference (`<sup class="mw-ref reference">`).
    ///
    /// `label` is the display number (e.g. `"1"`), `note_id` is the fragment
    /// identifying the entry in the reference list (e.g. `"cite_note-Foo-1"`).
    Ref { label: String, note_id: String },
}

/// An image extracted from a `<figure>` block in a Wikipedia article.
///
/// Wikipedia wraps images in `<figure>` elements containing an `<img>` and an
/// optional `<figcaption>`. Images appear between paragraphs, not inside them.
#[derive(Debug, Clone, Serialize)]
pub struct ImageSegment {
    /// Resolved URL of the image (thumbnail size as served by Wikimedia).
    pub src: String,
    /// Alt text from the `<img alt="...">` attribute.
    pub alt: String,
    /// Plain text of the `<figcaption>` element, if present.
    pub caption: String,
    /// The section heading path at the point where the image appears.
    pub section: String,
    /// The heading level of the current section (1–6). 0 if before any heading.
    pub section_level: u8,
}

/// A single item extracted from a Wikipedia article, in document order.
///
/// Paragraphs and images are interleaved as they appear in the source HTML,
/// so formatters can reproduce the original reading order. If references were
/// found, a single [`ArticleItem::References`] item is appended last.
#[derive(Debug, Clone)]
pub enum ArticleItem {
    /// A paragraph extracted from a `<p>` element.
    Paragraph(TextSegment),
    /// An image extracted from a `<figure>` element.
    Image(ImageSegment),
    /// All citation references collected from `<ol class="references">` lists.
    ///
    /// Keyed by the fragment id (e.g. `"cite_note-Foo-1"`), valued by the
    /// full plain-text citation string.
    References(HashMap<String, String>),
}

impl InlineNode {
    /// Returns the plain text content, stripping any markup.
    /// Returns an empty string for `Ref` nodes — citations are not prose text.
    pub fn plain_text(&self) -> &str {
        match self {
            InlineNode::Text(s) | InlineNode::Bold(s) | InlineNode::Italic(s) => s,
            InlineNode::Link { text, .. } => text,
            InlineNode::Ref { .. } => "",
        }
    }
}

/// A single paragraph-level text segment extracted from a Wikipedia article.
///
/// Each segment corresponds to a `<p>` block in the HTML. It captures the plain
/// text, the inline content structure, the MediaWiki paragraph ID, the section
/// heading path, and the heading depth.
#[derive(Debug, Clone, Serialize)]
pub struct TextSegment {
    /// The extracted plain text of this segment (inline markup stripped).
    pub text: String,
    /// The inline content nodes, preserving bold/italic/link structure.
    #[serde(skip)]
    pub content: Vec<InlineNode>,
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
/// use wikipedia_article_transform::{WikiPage, ArticleItem};
///
/// let mut page = WikiPage::new().unwrap();
/// let items = page.extract_text("<p>Hello.</p>").unwrap();
/// if let ArticleItem::Paragraph(seg) = &items[0] {
///     assert_eq!(seg.text, "Hello.");
/// }
/// ```
pub struct WikiPage {
    parser: Parser,
    items: Vec<ArticleItem>,
    current_sections: Vec<SectionInfo>,
    /// Base URL used to resolve relative hrefs, e.g. `https://en.wikipedia.org/wiki/`.
    base_url: Option<String>,
    /// Citation references collected by `extract_references()`.
    /// Keyed by note id (e.g. `"cite_note-Foo-1"`), valued by plain-text citation.
    references: HashMap<String, String>,
}

impl WikiPage {
    /// Creates a new `WikiPage`, initialising the tree-sitter HTML parser.
    pub fn new() -> anyhow::Result<Self> {
        let language = LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        Ok(WikiPage {
            parser,
            items: Vec::new(),
            current_sections: Vec::new(),
            base_url: None,
            references: HashMap::new(),
        })
    }

    /// Set the base URL for resolving relative link hrefs.
    ///
    /// Call this before [`extract_text`] when the HTML comes from a known origin.
    /// The `language` parameter is a Wikipedia language code (e.g. `"en"`, `"ml"`).
    ///
    /// ```rust
    /// use wikipedia_article_transform::WikiPage;
    ///
    /// let mut page = WikiPage::new().unwrap();
    /// page.set_base_url("en");
    /// ```
    pub fn set_base_url(&mut self, language: &str) {
        self.base_url = Some(format!("https://{language}.wikipedia.org/wiki/"));
    }

    /// Resolve an href against the base URL.
    ///
    /// - `./Foo`           → `{base}Foo`
    /// - `//en.wikipedia.org/wiki/Foo` → `https://en.wikipedia.org/wiki/Foo`
    /// - already `http(s)://` → unchanged
    /// - anything else (anchors, mw-data:, etc.) → unchanged
    fn resolve_href(&self, href: &str) -> String {
        if href.starts_with("http://") || href.starts_with("https://") {
            return href.to_string();
        }
        if let Some(rest) = href.strip_prefix("//") {
            return format!("https://{rest}");
        }
        if let Some(path) = href.strip_prefix("./") {
            if let Some(base) = &self.base_url {
                return format!("{base}{path}");
            }
        }
        href.to_string()
    }

    /// Parses `html` and returns one [`ArticleItem`] per paragraph or image, in document order.
    ///
    /// If any `<ol class="references">` lists are found, a final
    /// [`ArticleItem::References`] item is appended containing all citations.
    ///
    /// The parser state is reset on each call, so the same `WikiPage` can be
    /// reused safely across multiple articles.
    ///
    /// Skipped elements: `<script>`, `<style>`, `<link>`, and elements with
    /// classes `shortdescription`, `hatnote`, `infobox`, `reference`, `navbox`,
    /// `noprint`, `reflist`, `citation`.
    pub fn extract_text(&mut self, html: &str) -> anyhow::Result<Vec<ArticleItem>> {
        self.items.clear();
        self.current_sections.clear();
        self.references.clear();
        let tree = self
            .parser
            .parse(html, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse HTML"))?;
        let source = html.as_bytes();
        self.extract_references(&tree.root_node(), source);
        self.walk_and_collect(&tree.root_node(), source, false);
        if !self.references.is_empty() {
            self.items
                .push(ArticleItem::References(self.references.clone()));
        }
        Ok(self.items.clone())
    }

    /// Convenience method: parse `html` and return all paragraph text joined by `"\n\n"`.
    pub fn extract_text_plain(html: &str) -> anyhow::Result<String> {
        let mut page = WikiPage::new()?;
        let items = page.extract_text(html)?;
        let text = items
            .iter()
            .filter_map(|item| match item {
                ArticleItem::Paragraph(seg) => {
                    let t = seg.text.trim();
                    if t.is_empty() { None } else { Some(t) }
                }
                ArticleItem::Image(_) | ArticleItem::References(_) => None,
            })
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
        self.current_sections.last().map(|s| s.level).unwrap_or(0)
    }

    /// Pre-scan the parse tree for `<ol class="mw-references references">` elements
    /// and populate `self.references` with `note_id → citation_text` pairs.
    ///
    /// This runs before `walk_and_collect` so that inline `<sup>` nodes encountered
    /// during the main walk can be cross-referenced.
    fn extract_references(&mut self, node: &Node, source: &[u8]) {
        match node.kind() {
            "element" => {
                if let Some((tag, attrs)) = self.parse_element(node, source) {
                    let class = attrs
                        .iter()
                        .find(|(k, _)| k == "class")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("");
                    let classes: Vec<&str> = class.split_whitespace().collect();

                    // Found a reference list: collect its <li> children
                    if tag == "ol" && classes.contains(&"references") {
                        for child in node.children(&mut node.walk()) {
                            if child.kind() != "element" {
                                continue;
                            }
                            if let Some((child_tag, child_attrs)) =
                                self.parse_element(&child, source)
                            {
                                if child_tag != "li" {
                                    continue;
                                }
                                let note_id = child_attrs
                                    .iter()
                                    .find(|(k, _)| k == "id")
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or_default();
                                if note_id.is_empty() {
                                    continue;
                                }
                                // Find the <span class="mw-reference-text reference-text">
                                let citation = self.find_reference_text(&child, source);
                                if !citation.is_empty() {
                                    self.references.insert(note_id, citation);
                                }
                            }
                        }
                        return; // don't recurse into the ol further
                    }

                    // Recurse into other elements looking for more reference lists
                    for child in node.children(&mut node.walk()) {
                        self.extract_references(&child, source);
                    }
                }
            }
            _ => {
                for child in node.children(&mut node.walk()) {
                    self.extract_references(&child, source);
                }
            }
        }
    }

    /// Find and return the plain text of the `<span class="mw-reference-text">` inside a `<li>`.
    fn find_reference_text(&self, li_node: &Node, source: &[u8]) -> String {
        for child in li_node.children(&mut li_node.walk()) {
            if child.kind() != "element" {
                continue;
            }
            if let Some((tag, attrs)) = self.parse_element(&child, source) {
                let class = attrs
                    .iter()
                    .find(|(k, _)| k == "class")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                if tag == "span" && class.split_whitespace().any(|c| c == "reference-text") {
                    return self.extract_text_from_element(&child, source);
                }
                // Recurse — the span may be nested
                let found = self.find_reference_text(&child, source);
                if !found.is_empty() {
                    return found;
                }
            }
        }
        String::new()
    }

    /// Extract an [`InlineNode::Ref`] from a `<sup class="mw-ref reference">` node.
    ///
    /// Finds the inner `<a href="...#note_id">` for the note_id and the
    /// `<span class="mw-reflink-text">` for the display label.
    fn extract_inline_ref(&self, sup_node: &Node, source: &[u8]) -> Option<InlineNode> {
        let mut note_id = String::new();
        let mut label = String::new();

        self.find_ref_parts(sup_node, source, &mut note_id, &mut label);

        if note_id.is_empty() || label.is_empty() {
            return None;
        }
        Some(InlineNode::Ref { label, note_id })
    }

    /// Recursively walk a `<sup>` subtree collecting the anchor fragment (note_id)
    /// and the mw-reflink-text content (label).
    fn find_ref_parts(&self, node: &Node, source: &[u8], note_id: &mut String, label: &mut String) {
        for child in node.children(&mut node.walk()) {
            if child.kind() != "element" {
                continue;
            }
            if let Some((tag, attrs)) = self.parse_element(&child, source) {
                match tag.as_str() {
                    "a" => {
                        if note_id.is_empty() {
                            let href = attrs
                                .iter()
                                .find(|(k, _)| k == "href")
                                .map(|(_, v)| v.as_str())
                                .unwrap_or_default();
                            // href is like "./Article#cite_note-Foo-1" — take the fragment
                            if let Some(fragment) = href.rsplit_once('#') {
                                *note_id = fragment.1.to_string();
                            }
                        }
                        self.find_ref_parts(&child, source, note_id, label);
                    }
                    "span" => {
                        let class = attrs
                            .iter()
                            .find(|(k, _)| k == "class")
                            .map(|(_, v)| v.as_str())
                            .unwrap_or("");
                        if class.split_whitespace().any(|c| c == "mw-reflink-text") {
                            // Inner text is like "[1]" — strip the brackets
                            let raw = self.extract_text_from_element(&child, source);
                            *label = raw
                                .trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
                                .to_string();
                        } else {
                            self.find_ref_parts(&child, source, note_id, label);
                        }
                    }
                    _ => {
                        self.find_ref_parts(&child, source, note_id, label);
                    }
                }
            }
        }
    }

    /// Push an inline node onto the last text segment, also updating the plain text.
    fn push_inline(&mut self, node: InlineNode) {
        let last_seg = self.items.iter_mut().rev().find_map(|item| {
            if let ArticleItem::Paragraph(seg) = item {
                Some(seg)
            } else {
                None
            }
        });
        if let Some(seg) = last_seg {
            let plain = node.plain_text().to_string();
            if !seg.text.is_empty() && !plain.is_empty() {
                if !seg.text.ends_with(' ') {
                    seg.text.push(' ');
                }
            }
            seg.text.push_str(plain.trim());
            seg.content.push(node);
        }
    }

    /// Collect inline text from an element node into a single String (used for bold/italic).
    fn collect_inline_text(&self, node: &Node, source: &[u8]) -> String {
        let mut text = String::new();
        for child in node.children(&mut node.walk()) {
            match child.kind() {
                "text" => {
                    if let Ok(t) = child.utf8_text(source) {
                        let trimmed = t.trim();
                        if !trimmed.is_empty() {
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(trimmed);
                        }
                    }
                }
                "element" => {
                    let child_text = self.collect_inline_text(&child, source);
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

    fn walk_and_collect(&mut self, node: &Node, source: &[u8], inside_paragraph: bool) {
        match node.kind() {
            "text" => {
                if let Ok(text) = node.utf8_text(source) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        if self.items.is_empty() {
                            self.items.push(ArticleItem::Paragraph(TextSegment {
                                text: String::new(),
                                content: Vec::new(),
                                mwid: String::new(),
                                section: self.get_current_section_string(),
                                section_level: self.get_current_section_level(),
                            }));
                        }
                        self.push_inline(InlineNode::Text(trimmed.to_string()));
                    }
                }
            }
            "script_element" | "style_element" => (),
            "element" => {
                if let Some((tag_name, attributes)) = self.parse_element(node, source) {
                    if tag_name == "link" {
                        return;
                    }

                    let class_attr = attributes
                        .iter()
                        .find(|(k, _)| k == "class")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("");

                    // Handle citation refs before the class exclusion check:
                    // <sup class="mw-ref reference"> contains "reference" which would
                    // otherwise be excluded, but these are inline markers we want to keep.
                    if inside_paragraph
                        && tag_name == "sup"
                        && class_attr.split_whitespace().any(|c| c == "mw-ref")
                    {
                        if let Some(r) = self.extract_inline_ref(node, source) {
                            self.push_inline(r);
                        }
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
                        "mw-references",
                    ];
                    if EXCLUDED_CLASSES
                        .iter()
                        .any(|c| class_attr.split_whitespace().any(|cls| cls == *c))
                    {
                        return;
                    }

                    if let Some(level) = Self::get_header_level(&tag_name) {
                        let header_text = self.extract_text_from_element(node, source);
                        if !header_text.is_empty() {
                            self.update_sections(level, header_text);
                        }
                        return;
                    }

                    if tag_name == "p" {
                        let mwid = attributes
                            .iter()
                            .find(|(k, _)| k == "id")
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default();
                        self.items.push(ArticleItem::Paragraph(TextSegment {
                            text: String::new(),
                            content: Vec::new(),
                            mwid,
                            section: self.get_current_section_string(),
                            section_level: self.get_current_section_level(),
                        }));
                        for i in 0..node.child_count() {
                            if let Some(child) = node.child(i as u32) {
                                self.walk_and_collect(&child, source, true);
                            }
                        }
                        return;
                    }

                    if tag_name == "figure" {
                        if let Some(img) = self.extract_image(node, source) {
                            self.items.push(ArticleItem::Image(img));
                        }
                        return;
                    }

                    // Inline elements inside a paragraph
                    if inside_paragraph {
                        match tag_name.as_str() {
                            "b" | "strong" => {
                                let text = self.collect_inline_text(node, source);
                                if !text.is_empty() {
                                    self.push_inline(InlineNode::Bold(text));
                                }
                                return;
                            }
                            "i" | "em" => {
                                let text = self.collect_inline_text(node, source);
                                if !text.is_empty() {
                                    self.push_inline(InlineNode::Italic(text));
                                }
                                return;
                            }
                            "a" => {
                                let raw_href = attributes
                                    .iter()
                                    .find(|(k, _)| k == "href")
                                    .map(|(_, v)| v.as_str())
                                    .unwrap_or_default();
                                let href = self.resolve_href(raw_href);
                                let text = self.collect_inline_text(node, source);
                                if !text.is_empty() {
                                    self.push_inline(InlineNode::Link { text, href });
                                }
                                return;
                            }
                            _ => {}
                        }
                    }

                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i as u32) {
                            self.walk_and_collect(&child, source, inside_paragraph);
                        }
                    }
                }
            }
            _ => {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i as u32) {
                        self.walk_and_collect(&child, source, inside_paragraph);
                    }
                }
            }
        }
    }

    fn parse_element(
        &self,
        element_node: &Node,
        source: &[u8],
    ) -> Option<(String, Vec<(String, String)>)> {
        // Handle both normal elements (<tag>) and self-closing elements (<img/>)
        let tag_container = element_node
            .children(&mut element_node.walk())
            .find(|child| child.kind() == "start_tag" || child.kind() == "self_closing_tag")?;

        let tag_name_node = tag_container
            .children(&mut tag_container.walk())
            .find(|child| child.kind() == "tag_name")?;

        let tag_name = tag_name_node.utf8_text(source).ok()?.to_string();
        let mut attributes = Vec::new();

        for child in tag_container.children(&mut tag_container.walk()) {
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

    /// Extract an [`ImageSegment`] from a `<figure>` element node.
    ///
    /// Looks for a descendant `<img>` (self-closing) for `src`/`alt`, and a
    /// `<figcaption>` child for the caption text.
    fn extract_image(&self, figure_node: &Node, source: &[u8]) -> Option<ImageSegment> {
        let mut src = String::new();
        let mut alt = String::new();
        let mut caption = String::new();

        for child in figure_node.children(&mut figure_node.walk()) {
            if child.kind() == "element" {
                if let Some((tag, attrs)) = self.parse_element(&child, source) {
                    if tag == "figcaption" {
                        caption = self.extract_text_from_element(&child, source);
                    } else {
                        // Recurse into <a class="mw-file-description"> to find <img>
                        self.find_img(&child, source, &tag, &attrs, &mut src, &mut alt);
                    }
                }
            }
        }

        if src.is_empty() {
            return None;
        }

        Some(ImageSegment {
            src: self.resolve_href(&src),
            alt,
            caption,
            section: self.get_current_section_string(),
            section_level: self.get_current_section_level(),
        })
    }

    /// Recursively find the first `<img>` inside `node`, writing into `src`/`alt`.
    fn find_img(
        &self,
        node: &Node,
        source: &[u8],
        tag: &str,
        attrs: &[(String, String)],
        src: &mut String,
        alt: &mut String,
    ) {
        if !src.is_empty() {
            return;
        }
        if tag == "img" {
            if let Some((_, v)) = attrs.iter().find(|(k, _)| k == "src") {
                *src = v.clone();
            }
            if let Some((_, v)) = attrs.iter().find(|(k, _)| k == "alt") {
                *alt = v.clone();
            }
            return;
        }
        for child in node.children(&mut node.walk()) {
            if child.kind() == "element" {
                if let Some((child_tag, child_attrs)) = self.parse_element(&child, source) {
                    self.find_img(&child, source, &child_tag, &child_attrs, src, alt);
                }
            }
        }
    }
}

impl Default for WikiPage {
    fn default() -> Self {
        Self::new().expect("Failed to initialise tree-sitter HTML parser")
    }
}

/// Remove all reference-related content from a list of [`ArticleItem`]s.
///
/// Drops the [`ArticleItem::References`] item and removes [`InlineNode::Ref`]
/// nodes from every paragraph's content (also rebuilds the plain text).
/// Call this when `--include-references=false` is requested.
pub fn strip_references(items: Vec<ArticleItem>) -> Vec<ArticleItem> {
    items
        .into_iter()
        .filter_map(|item| match item {
            ArticleItem::References(_) => None,
            ArticleItem::Paragraph(mut seg) => {
                seg.content.retain(|n| !matches!(n, InlineNode::Ref { .. }));
                // Rebuild plain text without the ref labels
                seg.text = seg
                    .content
                    .iter()
                    .map(|n| n.plain_text())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                Some(ArticleItem::Paragraph(seg))
            }
            other => Some(other),
        })
        .collect()
}

/// Fetch a Wikipedia article by language code and title, returning article items in document order.
///
/// Requires the `fetch` feature.
#[cfg(any(feature = "cli", feature = "web"))]
pub async fn get_text(language: &str, title: &str) -> anyhow::Result<Vec<ArticleItem>> {
    let html = get_page_content_html(language, title).await?;
    let mut page = WikiPage::new()?;
    page.set_base_url(language);
    Ok(page.extract_text(&html)?)
}

#[cfg(any(feature = "cli", feature = "web"))]
async fn get_page_content_html(language: &str, title: &str) -> anyhow::Result<String> {
    let url = format!("https://{language}.wikipedia.org/api/rest_v1/page/html/{title}?stash=false");
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "wikipedia-article-transform/0.1 (https://github.com/santhoshtr/wikipedia-article-transform)",
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

    fn extract(html: &str) -> Vec<ArticleItem> {
        WikiPage::extract_text_plain(html).unwrap();
        let mut page = WikiPage::new().unwrap();
        page.extract_text(html).unwrap()
    }

    fn paragraphs(items: &[ArticleItem]) -> Vec<&TextSegment> {
        items
            .iter()
            .filter_map(|i| {
                if let ArticleItem::Paragraph(s) = i {
                    Some(s)
                } else {
                    None
                }
            })
            .collect()
    }

    fn images(items: &[ArticleItem]) -> Vec<&ImageSegment> {
        items
            .iter()
            .filter_map(|i| {
                if let ArticleItem::Image(s) = i {
                    Some(s)
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn test_basic_paragraph() {
        let items = extract("<html><body><p id=\"p1\">Hello world.</p></body></html>");
        let segs = paragraphs(&items);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Hello world.");
        assert_eq!(segs[0].mwid, "p1");
        assert_eq!(segs[0].section, "");
        assert_eq!(segs[0].section_level, 0);
    }

    #[test]
    fn test_multiple_paragraphs() {
        let items = extract("<p>First.</p><p>Second.</p><p>Third.</p>");
        let segs = paragraphs(&items);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "First.");
        assert_eq!(segs[1].text, "Second.");
        assert_eq!(segs[2].text, "Third.");
    }

    #[test]
    fn test_section_tracking() {
        let html = "<h2>History</h2><p>Para one.</p><h3>Early life</h3><p>Para two.</p>";
        let items = extract(html);
        let segs = paragraphs(&items);
        assert_eq!(segs[0].section, "History");
        assert_eq!(segs[1].section, "History - Early life");
    }

    #[test]
    fn test_section_level() {
        let html = "<h2>History</h2><p>A.</p><h3>Early life</h3><p>B.</p>";
        let items = extract(html);
        let segs = paragraphs(&items);
        assert_eq!(segs[0].section_level, 2);
        assert_eq!(segs[1].section_level, 3);
    }

    #[test]
    fn test_section_resets_at_same_level() {
        let html = "<h2>History</h2><p>A.</p><h2>Geography</h2><p>B.</p>";
        let items = extract(html);
        let segs = paragraphs(&items);
        assert_eq!(segs[0].section, "History");
        assert_eq!(segs[1].section, "Geography");
    }

    #[test]
    fn test_excluded_class_infobox() {
        let html = r#"<p>Visible.</p><table class="infobox"><tr><td>Hidden.</td></tr></table><p>Also visible.</p>"#;
        let items = extract(html);
        let segs = paragraphs(&items);
        assert!(segs.iter().all(|s| !s.text.contains("Hidden")));
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn test_excluded_class_reflist() {
        let html = r#"<p>Main text.</p><div class="reflist"><p>Ref text.</p></div>"#;
        let items = extract(html);
        let segs = paragraphs(&items);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Main text.");
    }

    #[test]
    fn test_script_and_style_skipped() {
        let html = "<p>Real.</p><script>var x=1;</script><style>body{}</style><p>Also real.</p>";
        let items = extract(html);
        let segs = paragraphs(&items);
        assert_eq!(segs.len(), 2);
        assert!(segs.iter().all(|s| !s.text.contains("var x")));
    }

    #[test]
    fn test_empty_html() {
        let items = extract("");
        assert!(items.is_empty());
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
        let items = page.extract_text("<p>Works.</p>").unwrap();
        let segs = paragraphs(&items);
        assert_eq!(segs[0].text, "Works.");
    }

    #[test]
    fn test_inline_bold() {
        let items = extract("<p><b>Bold</b> text</p>");
        let segs = paragraphs(&items);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Bold text");
        assert!(matches!(&segs[0].content[0], InlineNode::Bold(s) if s == "Bold"));
        assert!(matches!(&segs[0].content[1], InlineNode::Text(s) if s == "text"));
    }

    #[test]
    fn test_inline_italic() {
        let items = extract("<p><i>italic</i></p>");
        let segs = paragraphs(&items);
        assert_eq!(segs.len(), 1);
        assert!(matches!(&segs[0].content[0], InlineNode::Italic(s) if s == "italic"));
    }

    #[test]
    fn test_inline_strong_em() {
        let items = extract("<p><strong>S</strong> and <em>E</em></p>");
        let segs = paragraphs(&items);
        assert!(matches!(&segs[0].content[0], InlineNode::Bold(s) if s == "S"));
        assert!(matches!(&segs[0].content[2], InlineNode::Italic(s) if s == "E"));
    }

    #[test]
    fn test_inline_link() {
        let items = extract(r#"<p><a href="./X">anchor</a></p>"#);
        let segs = paragraphs(&items);
        assert_eq!(segs.len(), 1);
        // No base URL set: ./X passes through unchanged
        assert!(matches!(&segs[0].content[0],
            InlineNode::Link { text, href } if text == "anchor" && href == "./X"));
    }

    #[test]
    fn test_inline_link_absolute() {
        let html = r#"<p><a href="./Cryogenics">Cryogenics</a></p>"#;
        let mut page = WikiPage::new().unwrap();
        page.set_base_url("en");
        let items = page.extract_text(html).unwrap();
        let segs = paragraphs(&items);
        assert!(matches!(&segs[0].content[0],
            InlineNode::Link { text, href }
                if text == "Cryogenics"
                && href == "https://en.wikipedia.org/wiki/Cryogenics"));
    }

    #[test]
    fn test_resolve_href_protocol_relative() {
        let html = r#"<p><a href="//en.wikipedia.org/wiki/Oxygen">O</a></p>"#;
        let mut page = WikiPage::new().unwrap();
        let items = page.extract_text(html).unwrap();
        let segs = paragraphs(&items);
        assert!(matches!(&segs[0].content[0],
            InlineNode::Link { href, .. } if href == "https://en.wikipedia.org/wiki/Oxygen"));
    }

    #[test]
    fn test_format_plain_sections() {
        let html = "<p>Intro.</p><h2>History</h2><p>A.</p><h3>Early life</h3><p>B.</p>";
        let items = extract(html);
        let out = items.format_plain();
        assert!(out.contains("\nIntro.\n"), "intro paragraph missing");
        assert!(out.contains("## History\n"), "h2 heading missing");
        assert!(out.contains("\nA.\n"), "first section paragraph missing");
        assert!(out.contains("### Early life\n"), "h3 heading missing");
        assert!(out.contains("\nB.\n"), "subsection paragraph missing");
        assert!(out.find("## History").unwrap() < out.find("\nA.\n").unwrap());
        assert!(out.find("### Early life").unwrap() < out.find("\nB.\n").unwrap());
    }

    #[test]
    fn test_format_json_tree() {
        let html = "<p>Intro.</p><h2>History</h2><p>A.</p><h3>Early life</h3><p>B.</p>";
        let items = extract(html);
        let json_str = items.format_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["intro"][0], "Intro.");
        assert_eq!(v["sections"][0]["heading"], "History");
        assert_eq!(v["sections"][0]["level"], 2);
        assert_eq!(v["sections"][0]["paragraphs"][0], "A.");
        assert_eq!(v["sections"][0]["subsections"][0]["heading"], "Early life");
        assert_eq!(v["sections"][0]["subsections"][0]["level"], 3);
        assert_eq!(v["sections"][0]["subsections"][0]["paragraphs"][0], "B.");
    }

    #[test]
    fn test_format_markdown_inline() {
        let items = extract(
            "<h2>Title</h2><p><b>Bold</b> and <i>italic</i> and <a href=\"/x\">link</a></p>",
        );
        let out = items.format_markdown();
        assert!(out.contains("## Title"));
        assert!(out.contains("**Bold**"));
        assert!(out.contains("_italic_"));
        assert!(out.contains("[link](/x)"));
        // spaces between inline nodes must be preserved
        assert!(
            out.contains("**Bold** and"),
            "space after bold missing: {out}"
        );
        assert!(
            out.contains("_italic_ and"),
            "space after italic missing: {out}"
        );
        assert!(
            out.contains("and [link]"),
            "space before link missing: {out}"
        );
    }

    #[test]
    fn test_image_extraction() {
        let html = r#"<figure typeof="mw:File/Thumb">
            <a href="./File:Foo.jpg" class="mw-file-description">
                <img alt="A description" src="//upload.wikimedia.org/thumb/foo.jpg" class="mw-file-element"/>
            </a>
            <figcaption>Caption text here.</figcaption>
        </figure>"#;
        let items = extract(html);
        let imgs = images(&items);
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].src, "https://upload.wikimedia.org/thumb/foo.jpg");
        assert_eq!(imgs[0].alt, "A description");
        assert_eq!(imgs[0].caption, "Caption text here.");
    }

    #[test]
    fn test_image_no_caption() {
        let html = r#"<figure typeof="mw:File/Frameless">
            <a href="./File:Bar.png" class="mw-file-description">
                <img alt="Bar" src="//upload.wikimedia.org/bar.png" class="mw-file-element"/>
            </a>
            <figcaption></figcaption>
        </figure>"#;
        let items = extract(html);
        let imgs = images(&items);
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].caption, "");
    }

    #[test]
    fn test_image_section_tracking() {
        let html = r#"<h2>History</h2>
        <figure typeof="mw:File/Thumb">
            <a href="./File:X.jpg"><img alt="X" src="//upload.wikimedia.org/x.jpg"/></a>
            <figcaption>X caption</figcaption>
        </figure>
        <p>A paragraph.</p>"#;
        let items = extract(html);
        let imgs = images(&items);
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].section, "History");
        assert_eq!(imgs[0].section_level, 2);
    }

    #[test]
    fn test_image_interleaved_order() {
        let html = r#"<p>Before.</p>
        <figure typeof="mw:File/Thumb">
            <a href="./File:X.jpg"><img alt="X" src="//upload.wikimedia.org/x.jpg"/></a>
            <figcaption>Caption</figcaption>
        </figure>
        <p>After.</p>"#;
        let items = extract(html);
        assert!(matches!(&items[0], ArticleItem::Paragraph(s) if s.text == "Before."));
        assert!(matches!(&items[1], ArticleItem::Image(_)));
        assert!(matches!(&items[2], ArticleItem::Paragraph(s) if s.text == "After."));
    }

    #[test]
    fn test_markdown_image() {
        let html = r#"<figure typeof="mw:File/Thumb">
            <a href="./File:Foo.jpg"><img alt="Alt text" src="//upload.wikimedia.org/foo.jpg"/></a>
            <figcaption>The caption.</figcaption>
        </figure>"#;
        let items = extract(html);
        let out = items.format_markdown();
        // caption is used as alt text intentionally
        assert!(out.contains("![Alt text](https://upload.wikimedia.org/foo.jpg)"));
        assert!(out.contains("_The caption._"));
    }

    #[test]
    fn test_markdown_image_no_caption() {
        let html = r#"<figure typeof="mw:File/Frameless">
            <a href="./File:Bar.png"><img alt="Bar" src="//upload.wikimedia.org/bar.png"/></a>
            <figcaption></figcaption>
        </figure>"#;
        let items = extract(html);
        let out = items.format_markdown();
        assert!(out.contains("![Bar](https://upload.wikimedia.org/bar.png)"));
        // no caption line when caption is empty
        assert!(!out.contains("__"));
    }

    #[test]
    fn test_plain_image() {
        let html = r#"<figure typeof="mw:File/Thumb">
            <a href="./File:Foo.jpg"><img alt="Alt text" src="//upload.wikimedia.org/foo.jpg"/></a>
            <figcaption>The caption.</figcaption>
        </figure>"#;
        let items = extract(html);
        let out = items.format_plain();
        assert!(out.contains("[Image: Alt text]"));
        assert!(out.contains("The caption."));
    }

    #[test]
    fn test_json_image() {
        let html = r#"<h2>Section</h2>
        <figure typeof="mw:File/Thumb">
            <a href="./File:Foo.jpg"><img alt="Alt text" src="//upload.wikimedia.org/foo.jpg"/></a>
            <figcaption>The caption.</figcaption>
        </figure>
        <p>A paragraph.</p>"#;
        let items = extract(html);
        let json_str = items.format_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["sections"][0]["images"][0]["alt"], "Alt text");
        assert_eq!(
            v["sections"][0]["images"][0]["src"],
            "https://upload.wikimedia.org/foo.jpg"
        );
        assert_eq!(v["sections"][0]["images"][0]["caption"], "The caption.");
    }

    // ── Reference tests ─────────────────────────────────────────────────────

    fn ref_html() -> &'static str {
        r#"<p id="p1">Some text.<sup class="mw-ref reference" typeof="mw:Extension/ref"
            ><a href="./Article#cite_note-Foo-1"><span class="mw-reflink-text">[1]</span></a
            ></sup> More text.<sup class="mw-ref reference" typeof="mw:Extension/ref"
            ><a href="./Article#cite_note-Bar-2"><span class="mw-reflink-text">[2]</span></a
            ></sup></p>
        <ol class="mw-references references">
            <li id="cite_note-Foo-1" data-mw-footnote-number="1">
                <span class="mw-cite-backlink"><a href="./Article#cite_ref-Foo_1-0">↑</a></span>
                <span id="mw-reference-text-cite_note-Foo-1" class="mw-reference-text reference-text">Author A. <i>Title One</i>. Publisher, 2020.</span>
            </li>
            <li id="cite_note-Bar-2" data-mw-footnote-number="2">
                <span class="mw-cite-backlink"><a href="./Article#cite_ref-Bar_2-0">↑</a></span>
                <span id="mw-reference-text-cite_note-Bar-2" class="mw-reference-text reference-text">Author B. Title Two. Journal, 2021.</span>
            </li>
        </ol>"#
    }

    #[test]
    fn test_ref_inline_nodes() {
        let items = extract(ref_html());
        let segs = paragraphs(&items);
        assert_eq!(segs.len(), 1);
        // content: Text, Ref[1], Text, Ref[2]
        assert!(matches!(&segs[0].content[0], InlineNode::Text(s) if s.contains("Some text")));
        assert!(
            matches!(&segs[0].content[1], InlineNode::Ref { label, note_id }
            if label == "1" && note_id == "cite_note-Foo-1")
        );
        assert!(
            matches!(&segs[0].content[3], InlineNode::Ref { label, note_id }
            if label == "2" && note_id == "cite_note-Bar-2")
        );
    }

    #[test]
    fn test_ref_plain_text_excludes_label() {
        // plain_text() on Ref returns "" so the ref label is not in seg.text
        let items = extract(ref_html());
        let segs = paragraphs(&items);
        assert!(!segs[0].text.contains('['));
        assert!(segs[0].text.contains("Some text"));
        assert!(segs[0].text.contains("More text"));
    }

    #[test]
    fn test_ref_references_item_appended() {
        let items = extract(ref_html());
        let refs = items.iter().find_map(|i| {
            if let ArticleItem::References(r) = i {
                Some(r)
            } else {
                None
            }
        });
        assert!(refs.is_some());
        let refs = refs.unwrap();
        assert_eq!(refs.len(), 2);
        assert!(refs["cite_note-Foo-1"].contains("Title One"));
        assert!(refs["cite_note-Bar-2"].contains("Title Two"));
    }

    #[test]
    fn test_ref_no_refs_no_item() {
        let items = extract("<p>No citations here.</p>");
        assert!(
            !items
                .iter()
                .any(|i| matches!(i, ArticleItem::References(_)))
        );
    }

    #[test]
    fn test_ref_markdown_inline_and_list() {
        let items = extract(ref_html());
        let out = items.format_markdown();
        // Inline labels appear attached to surrounding text
        assert!(out.contains("[^1]"), "inline [^1] missing");
        assert!(out.contains("[^2]"), "inline [^2] missing");
        // Reference definitions at the bottom
        assert!(out.contains("## References"), "References heading missing");
        assert!(out.contains("[^1]: "), "[^1]: definition missing");
        assert!(out.contains("Title One"), "citation text missing");
        assert!(out.contains("[^2]: "), "[^2]: definition missing");
        assert!(out.contains("Title Two"), "citation text missing");
        // Definitions must appear after body text
        assert!(out.find("Some text").unwrap() < out.find("## References").unwrap());
    }

    #[test]
    fn test_ref_json_references_key() {
        let items = extract(ref_html());
        let json_str = items.format_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(v["references"].is_object(), "references key missing");
        assert!(
            v["references"]["cite_note-Foo-1"]
                .as_str()
                .unwrap()
                .contains("Title One")
        );
        assert!(
            v["references"]["cite_note-Bar-2"]
                .as_str()
                .unwrap()
                .contains("Title Two")
        );
    }

    #[test]
    fn test_strip_references() {
        let items = extract(ref_html());
        let stripped = strip_references(items);
        // No References item
        assert!(
            !stripped
                .iter()
                .any(|i| matches!(i, ArticleItem::References(_)))
        );
        // No Ref inline nodes in paragraphs
        let segs = paragraphs(&stripped);
        for seg in segs {
            assert!(
                !seg.content
                    .iter()
                    .any(|n| matches!(n, InlineNode::Ref { .. }))
            );
            assert!(!seg.text.contains('['));
        }
    }
}
