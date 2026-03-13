# wikipedia-article-transform

Extract plain text from Wikipedia article HTML using [tree-sitter](https://tree-sitter.github.io/).

Parses the full HTML structure of a Wikipedia article and returns clean prose text — skipping
infoboxes, navigation elements, references, hatnotes, and other non-content markup. Section
headings are tracked so each paragraph knows which section it belongs to.

## Usage

### As a library

```toml
[dependencies]
wikipedia-article-transform = "0.1"
```

**Simple: get all text as a string**

```rust
use wiki_html_text_extractor::WikiPage;

let html = r#"<h2>History</h2><p id="p1">Some text here.</p>"#;
let text = WikiPage::extract_text_plain(html)?;
// "Some text here."
```

**Structured: get paragraphs with section and tag context**

```rust
use wiki_html_text_extractor::WikiPage;

let mut page = WikiPage::new()?;
let segments = page.extract_text(html)?;

for seg in &segments {
    println!("[{}] {}", seg.section, seg.text);
}
```

Reuse the same `WikiPage` across many articles — it resets state internally on each call
and avoids reinitialising the parser.

**Filtering by context**

```rust
// Only paragraphs inside a specific section
let prose: Vec<_> = segments.iter()
    .filter(|s| s.section.starts_with("History"))
    .collect();

// Skip paragraphs inside a table
let no_tables: Vec<_> = segments.iter()
    .filter(|s| !s.is_in_tag("table"))
    .collect();

// Match a CSS-like selector path
let main_content: Vec<_> = segments.iter()
    .filter(|s| s.is_in_tag_with_class("mw-body"))
    .collect();
```

### With the `fetch` feature

Fetch and extract a live Wikipedia article directly:

```toml
[dependencies]
wikipedia-article-transform = { version = "0.1", features = ["fetch"] }
```

```rust
use wiki_html_text_extractor::get_text;

let segments = get_text("en", "Rust_(programming_language)").await?;
```

## CLI

Install with the `fetch` feature (required for the binary):

```sh
cargo install wikipedia-article-transform --features fetch
```

**Fetch an article:**

```sh
wikipedia-article-transform fetch --language en --title "Rust_(programming_language)"
wikipedia-article-transform fetch --language ml --title "കേരളം" --format json
```

**Process a NDJSON stream (for bulk pipeline use):**

```sh
# Input:  {"id": 123, "url": "...", "name": "...", "html": "..."}
# Output: {"id": 123, "url": "...", "name": "...", "text": "..."}
cat articles.ndjson | wikipedia-article-transform stdin
```

## Skipped elements

The following are excluded from extracted text:

| Element / class | Reason |
|---|---|
| `<script>`, `<style>` | Code, not prose |
| `<link>` | Metadata |
| `.infobox` | Structured data table |
| `.reflist`, `.reference`, `.citation` | Reference list |
| `.navbox` | Navigation template |
| `.hatnote` | Disambiguation notice |
| `.shortdescription` | Hidden metadata |
| `.noprint` | Print-only elements |

## Feature flags

| Feature | Default | Description |
|---|---|---|
| `fetch` | no | Enables `get_text()` and the CLI binary (adds `reqwest` + `tokio`) |

## License

MIT
