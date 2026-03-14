---
name: wikipedia-article-transform
description: Fetch and extract clean, structured text from any Wikipedia article using the wikipedia-article-transform CLI. Use this skill whenever the user asks what Wikipedia says about a topic, requests a Wikipedia article summary, wants section-wise extraction, or needs article content in plain text, markdown, or JSON for further analysis. Use proactively for multilingual Wikipedia tasks (en, ml, fr, de, etc.) even when the user does not explicitly mention the CLI tool.
---

# Wikipedia Article Transform CLI

Use this skill to fetch and transform Wikipedia article content through the `wikipedia-article-transform` command.

## Core behavior

- Prefer `uvx wikipedia-article-transform ...` first. This runs the tool without explicit installation.
- If `uvx` is unavailable, install and use one of these:
  - `pip install wikipedia-article-transform` then run `wikipedia-article-transform ...`
  - `cargo install wikipedia-article-transform --features cli` then run `wikipedia-article-transform ...`
- Default to English (`en`) when language is not specified.
- Preserve user-requested language when explicitly stated.

## Output format selection

Infer format without asking unless the user is ambiguous in a way that changes the output materially.

- Use `markdown` for summaries, readable reports, and quote-friendly output.
- Use `json` for programmatic analysis, counting sections, extracting structure, downstream automation, or any request that explicitly asks for citations with content. JSON includes per-paragraph `citations` arrays for reliable attribution.
- Use `plain` for plain-text pipelines or copy/paste workflows that should avoid markup.

## Command template

```bash
uvx wikipedia-article-transform fetch \
  --language <lang> \
  --title "<title>" \
  --format <plain|markdown|json>
```

Options:

- `--language` / `-l`: Wikipedia language code (`en`, `ml`, `fr`, etc.)
- `--title` / `-t`: article title (quote when containing spaces)
- `--format` / `-f`: `plain`, `markdown`, or `json`
- `--include-references` / `--no-include-references`: include or omit citations in output

## Title and language handling

- Keep native script titles as-is (Unicode titles are supported).
- Quote titles with spaces in shell commands.
- Prefer exact Wikipedia title casing when known.
- Map language mentions to codes directly (for example, Malayalam -> `ml`, French -> `fr`).

## Error handling

- If command not found:
  1) check `uvx` availability,
  2) fallback to installation path,
  3) retry command.
- If fetch returns HTTP 404, retry with a corrected title variant.
- If output is too large for inline display, summarize and keep the raw output in file form when appropriate.

## Example commands

```bash
uvx wikipedia-article-transform fetch --language en --title "Liquid oxygen" --format markdown
uvx wikipedia-article-transform fetch --language ml --title "കേരളം" --format json
uvx wikipedia-article-transform fetch --language en --title "Marie Curie" --format plain --include-references
```

## Output reference

When explaining output structure, read `references/output-formats.md`.

## Citation-focused answers

- If the user asks for content with citations, prefer `--format json`.
- Build the answer from paragraph `text` plus each paragraph's `citations` array.
- Do not return bare citation numbers like `[^12]` without resolved citation text.
