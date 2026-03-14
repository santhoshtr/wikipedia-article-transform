# Output Formats

The tool supports three output formats: `plain`, `markdown`, and `json`.

## plain

- Readable text with heading lines.
- Section headings render as markdown-style hashes (for example `## History`).
- References are omitted from plain output.

Example snippet:

```text
Oxygen

Oxygen is a chemical element ...

## History of study

The modern concept of the element oxygen ...
```

## markdown

- Preserves inline formatting for emphasis and links.
- Uses footnote-style citations when references are included.
- Appends a `## References` section with `[^N]: ...` entries.

Example snippet:

```markdown
**Oxygen** is a [chemical element](https://en.wikipedia.org/wiki/Chemical_element) ...[^7]

## History of study

The modern concept of the element oxygen ...

## References

[^7]: Full citation text
```

## json

- Structured tree for machine use.
- Paragraph entries include resolved per-paragraph citations.
- Top-level fields:
  - `intro`: list of paragraph objects
  - `intro_images`: list of intro images
  - `sections`: recursive section tree
  - `references`: map of citation ids to full citation text

Example snippet:

```json
{
  "intro": [
    {
      "text": "Oxygen is a chemical element ...",
      "citations": [
        { "label": "7", "text": "Full citation text ..." }
      ]
    }
  ],
  "intro_images": [],
  "sections": [
    {
      "heading": "History of study",
      "level": 2,
      "paragraphs": [
        {
          "text": "The modern concept ...",
          "citations": []
        }
      ],
      "images": [],
      "subsections": []
    }
  ],
  "references": {
    "cite_note-Foo-1": "Full citation text ..."
  }
}
```
