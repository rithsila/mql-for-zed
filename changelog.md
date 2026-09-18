# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `scripts/mql-compile-helper.swift` — native arm64 Swift binary that initialises `NSApplication` before spawning Wine, providing the Cocoa event loop required by Wine's macOS display driver (`winemac.drv`) for headless MetaEditor compilation.
- `implement_plan.md` — detailed implementation plan for the MQL compile feature, covering a local Wine backend (macOS + Rosetta 2) and a remote Windows VM backend, shared log parser design, Zed task integration, and path-translation strategy.


## [0.1.0] - 2026-09-18

### Added

- Initial release of MQL (MetaQuotes Language) extension for Zed editor.
- Tree-sitter grammar for MQL4/MQL5 syntax parsing.
- Syntax highlighting via `highlights.scm`.
- Code outline support via `outline.scm`.
- Bracket matching via `brackets.scm`.
- Language configuration (`config.toml`) with comment and bracket definitions.
- Compiled `extension.wasm` for Zed extension loading.
