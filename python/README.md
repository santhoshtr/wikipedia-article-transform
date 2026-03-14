# wikipedia-article-transform (Python)

Python bindings for the Rust `wikipedia-article-transform` library.

## Install (from source)

```sh
pip install maturin
maturin develop --release
```

## Library usage

```python
from wikipedia_article_transform import fetch_article_html, extract

html = fetch_article_html("en", "Rust_(programming_language)")
text = extract(html, format="plain", language="en")
print(text)
```

## CLI usage

```sh
wikipedia-article-transform fetch --language en --title "Rust_(programming_language)"
wikipedia-article-transform fetch --language ml --title "കേരളം" --format json
wikipedia-article-transform fetch --language en --title "Liquid_oxygen" --format markdown
```
