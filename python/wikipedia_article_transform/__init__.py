from ._native import extract, extract_json, extract_markdown, extract_plain  # type: ignore[import-not-found]
from .client import fetch_article_html

__all__ = [
    "extract",
    "extract_plain",
    "extract_markdown",
    "extract_json",
    "fetch_article_html",
]
