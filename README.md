# wikipedia-article-transform

Extract plain text from Wikipedia article HTML using [tree-sitter](https://tree-sitter.github.io/).

Parses the full HTML structure of a Wikipedia article and returns clean prose text — skipping
infoboxes, navigation elements, references, hatnotes, and other non-content markup. Section
headings are tracked so each paragraph knows which section it belongs to, and inline elements
(`<b>`, `<i>`, `<a>`) are captured for rich output formats.

## Usage

### As a library

```toml
[dependencies]
wikipedia-article-transform = "0.1"
```

**Simple: get all text as a string**

```rust
use wikipedia_article_transform::WikiPage;

let html = r#"<h2>History</h2><p id="p1">Some text here.</p>"#;
let text = WikiPage::extract_text_plain(html)?;
// "Some text here."
```

**Structured: get paragraphs with section context**

```rust
use wikipedia_article_transform::WikiPage;

let mut page = WikiPage::new()?;
let segments = page.extract_text(html)?;

for seg in &segments {
    println!("[{}] {}", seg.section, seg.text);
}
```

Reuse the same `WikiPage` across many articles — it resets state internally on each call
and avoids reinitialising the parser.

**Filtering by section**

```rust
let prose: Vec<_> = segments.iter()
    .filter(|s| s.section.starts_with("History"))
    .collect();
```

**Output formatting**

```rust
use wikipedia_article_transform::ArticleFormat;

// Plain text with # heading lines
let plain = segments.format_plain();

// Semantic JSON: { "intro": [...], "sections": [{ "heading": "...", "level": 2, ... }] }
let json = segments.format_json()?;

// Markdown with **bold**, _italic_, [links](href)
let markdown = segments.format_markdown();
```

### With the `fetch` feature

Fetch and extract a live Wikipedia article directly:

```toml
[dependencies]
wikipedia-article-transform = { version = "0.1", features = ["cli"] }
```

```rust
use wikipedia_article_transform::{get_text, ArticleFormat};

let segments = get_text("en", "Rust_(programming_language)").await?;
println!("{}", segments.format_markdown());
```

## CLI

Install with the `fetch` feature (required for the binary):

```sh
cargo install wikipedia-article-transform --features cli
```

**Fetch an article:**

```sh
# Plain text (default)
wiki-html-text-extractor fetch --language en --title "Rust_(programming_language)"

# Semantic JSON section tree
wiki-html-text-extractor fetch --language ml --title "കേരളം" --format json

# Markdown with inline formatting
wiki-html-text-extractor fetch --language en --title "Liquid_oxygen" --format markdown
```

## JSON output shape

```json
{
  "intro": ["Paragraphs before the first heading..."],
  "sections": [
    {
      "heading": "Safety and precautions",
      "level": 2,
      "paragraphs": ["Overview text..."],
      "subsections": [
        {
          "heading": "Combustion and other hazards",
          "level": 3,
          "paragraphs": ["Liquid oxygen spills..."],
          "subsections": []
        }
      ]
    }
  ]
}
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
| `cli` | no | Enables `get_text()` and the CLI binary (adds `reqwest` + `tokio`) |

## License

MIT
