# wikipedia-article-transform

[![Cargo](https://img.shields.io/crates/v/wikipedia-article-transform.svg)](https://crates.io/crates/wikipedia-article-transform)
[![PyPI](https://img.shields.io/pypi/v/wikipedia-article-transform.svg)](https://pypi.org/project/wikipedia-article-transform/)

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

### Python bindings

Build locally with `maturin`:

```sh
cd python
pip install maturin
maturin develop --release
```

Use in Python:

```python
from wikipedia_article_transform import fetch_article_html, extract

html = fetch_article_html("en", "Rust_(programming_language)")
print(extract(html, format="markdown", language="en"))
```

## CLI

Install with the `fetch` feature (required for the binary):

```sh
cargo install wikipedia-article-transform --features cli
```

**Fetch an article:**

```sh
# Plain text (default)
wikipedia-article-transform fetch --language en --title "Rust_(programming_language)"

# Semantic JSON section tree
wikipedia-article-transform fetch --language ml --title "കേരളം" --format json

# Markdown with inline formatting
wikipedia-article-transform fetch --language en --title "Liquid_oxygen" --format markdown
```

### Python CLI

After installing the Python package, use the same command shape:

```sh
wikipedia-article-transform fetch --language en --title "Rust_(programming_language)"
wikipedia-article-transform fetch --language ml --title "കേരളം" --format json
wikipedia-article-transform fetch --language en --title "Liquid_oxygen" --format markdown
```

## Web API (`web` feature)

Run the HTTP API server:

```sh
cargo run --features web --bin wikipedia-article-transform-web
```

Routes:

```text
GET /healthz
GET /{language}/{title}.md
GET /{language}/{title}.txt
GET /{language}/{title}.json
```

Examples:

```sh
curl "http://localhost:10000/en/Oxygen.md"
curl "http://localhost:10000/en/Oxygen.txt"
curl "http://localhost:10000/en/Oxygen.json"
```

The server binds to `0.0.0.0:$PORT` (`PORT` defaults to `10000`) and sets output-specific content types.

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
| `web` | no | Enables the Actix API server binary (`wikipedia-article-transform-web`) |

## License

MIT
