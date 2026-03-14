from __future__ import annotations

import argparse
import sys

from ._native import extract  # type: ignore[import-not-found]
from .client import fetch_article_html


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Extract plain text from Wikipedia HTML"
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    fetch = subparsers.add_parser(
        "fetch",
        help="Fetch a Wikipedia article by language and title, print extracted text.",
    )
    fetch.add_argument(
        "-l",
        "--language",
        required=True,
        help='Wikipedia language code (e.g. "en", "ml")',
    )
    fetch.add_argument("-t", "--title", required=True, help="Wikipedia article title")
    fetch.add_argument(
        "-f",
        "--format",
        choices=("plain", "json", "markdown"),
        default="plain",
        help="Output format",
    )
    fetch.add_argument(
        "--include-references",
        default=True,
        action=argparse.BooleanOptionalAction,
        help="Include citation references inline and as a reference list at the end",
    )

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)

    if args.command == "fetch":
        try:
            html = fetch_article_html(args.language, args.title)
            out = extract(
                html,
                format=args.format,
                language=args.language,
                include_references=args.include_references,
            )
            print(out)
            return 0
        except RuntimeError as exc:
            print(f"Error: {exc}", file=sys.stderr)
            return 1

    parser.print_help()
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
