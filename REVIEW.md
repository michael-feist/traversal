# Review: lnk→tgt DocumentLink resolution (`traversal-lsp/main.rs`)

Reviewed the `textDocument/documentLink` handler at
`crates/traversal-lsp/src/bin/traversal-lsp/main.rs:275-331`.

## What's good
- Resolves each `traverse-lnk` to its matching `traverse-tgt` by id
  (`target_tags.tag_indices_by_id[link_tag.id]`).
- Converts 1-based grep line numbers to 0-based LSP `Position.line`
  via `saturating_sub(1)`.

## Bug 1 (major): `range` is the target's location, not the requesting document's
`DocumentLink.range` is always measured in the document named in the request
(`document_link_req.text_document.uri`). The handler sets it from `target_tag`
(`main.rs:298-307`). In the fixture, every one of the 41 links is cross-file, so
the underline is placed at the target's line number inside the current file,
highlighting an unrelated line. The range must stay on the link annotation
(`link_tag` positions, 0-based line); only the `target` URI should point at the tgt.

## Bug 2: target fragment uses the 0-based line
`main.rs:313-314` reuses `line_number` (already decremented) in `#L{},{}`.
VSCode parses `#L{n}` fragments as 1-based, so Ctrl+click lands one line above
the target. Use `target_tag.line_number` for the fragment; keep the 0-based
value only for `Position.line`.

## Bug 3: panic on dangling links
`main.rs:290-292` indexes `tag_indices_by_id[link_tag.id.as_str()]`, which
panics if the id has no target. All ids currently resolve, but a stray
`[traverse-lnk: foo]` would crash the server (panics bypass the
`if let Err(...)` handler). Use `.get()` + `if let Some(...)` and emit the link
targetless (or skip it).

## Minor
- Fragment column is off by one: `target_tag.range.start` is 0-based, VSCode
  fragment columns are 1-based.
- Range columns are byte offsets into the line (`group.range()`) and are only
  correct for ASCII; non-ASCII annotation lines would misalign under UTF-16
  position encoding. Non-blocking for the current fixture.

## Proposed corrected shape
- `range` = `link_tag` positions (current doc, 0-based line).
- `target` = `file://{target_tag.file_path}#L{target_tag.line_number},{range.start}`.
- Lookups via `.get()`/`if let`.
- Rebuild + `cargo install --path crates/traversal-lsp`; verify with
  `"languageServerExample.trace.server": "verbose"` that `file_0007.go` returns
  ranges on the annotation lines (0-based 4/72/81/91) with cross-file targets.