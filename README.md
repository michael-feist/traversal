# traversal

Traversal is a set of tools that make your codebase more navigable. Create links between arbitrary file types allowing you to cross-reference almost any part of your codebase regardless of language choice.

<insert GIF of traversal in action>

## Quickstart

<add installation instructions and a short example of how to use traversal>

## Overview

Traversal works by finding "tags" in your codebase. They look something like this: `[traverse-tgt: foo]`.

There are two kinds of tags:
- Target tags: Places to link to (`[traverse-tgt: foo]`)
- Link tags: Place to link from (`[traverse-lnk: foo]`)

Traversal searches your code base for tags by a dumb match and can resolve them on request. Since all Traversal is doing is looking for the tag pattern, tags can be added to any type of file (for the most part).

## Tools

- `traversal`: A CLI tool that finds all tags in the given folder and prints them.
- `traversal-lsp`: An LSP server that resolves link tags to their corresponding target tag.
- Traversal VSCode extension: A VSCode extension integrating `traversal-lsp` with VSCode.

## License

This project is licensed under the [MIT License](./LICENSE).
