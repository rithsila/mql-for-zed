# MQL Smart Hover — Implementation Plan

> Status: proposed; no runtime feature implemented by this document.
> Created: 2026-10-13
> Scope: MQL5 code understanding in Zed, with independently configurable English and Khmer explanations.

## Objective

Hover over a symbol to understand its declaration, behavior, usage context, and direct relationships without leaving the editor. Keep source-derived facts separate from documentation and AI interpretation. Help the developer verify AI-generated code; never imply trading safety or profitability from an explanation.

All checklist items below are pending. Proposed settings, modules, schemas, and MCP tools are not existing APIs.

## Existing foundation

- `src/lib.rs`: forwards `lsp.mql-lsp.initialization_options` from Zed to the language server.
- `lsp/src/main.rs`: advertises hover, completion, and definitions; renders signatures and comments as Markdown. Hover resolves a word by name and displays up to five matches.
- `lsp/src/index.rs`: regex-based indexing of functions, classes/structs, enums, enum members, and macros. The symbol record has a name, kind, signature, documentation, and declaration file/line; it has no structured scope or relationship model.
- `lsp/data/builtins.json`: bundled built-in signatures and documentation.
- `.set` hover has a separate request path showing the associated input's type and default.
- `mql-mcp/`: existing agent integration; future symbol tools must reuse the same analysis model rather than invent a second resolver.

## Product decisions

1. Offline, deterministic facts are the foundation; AI is optional and off by default.
2. English and Khmer can each be enabled or disabled independently.
3. A master switch disables this server's hover output without disabling completion, definitions, or diagnostics.
4. Both languages disabled means signature/facts-only hover, not implicitly disabling hover.
5. Code, identifiers, paths, and signatures are never translated.
6. Default behavior remains English-only for compatibility. Bilingual and Khmer-only configurations are explicit choices.
7. Keep the popup compact. Detailed walkthroughs belong in a generated Markdown document or Agent conversation.
8. Never guess a declaration when scope resolution is ambiguous. Show identified candidates or an unresolved state.
9. Hover must never require network access, invoke a compiler, or execute workspace code.

## Configuration contract

### Proposed complete MVP configuration

Place this under the existing Zed settings structure, preserving other `initialization_options` such as `mql5Path`, `lint`, and `compilerCheck`:

```json
{
  "lsp": {
    "mql-lsp": {
      "initialization_options": {
        "hover": {
          "enabled": true,
          "languages": {
            "english": true,
            "khmer": false
          },
          "primaryLanguage": "en",
          "showSource": true,
          "showDocumentation": true
        }
      }
    }
  }
}
```

### Fields and defaults

| Field | Default | Meaning |
|---|---|---|
| `hover.enabled` | `true` | Master switch for all hover responses from `mql-lsp`, including `.set` hover |
| `hover.languages.english` | `true` | Show English explanation/documentation blocks |
| `hover.languages.khmer` | `false` | Show Khmer explanation/documentation blocks |
| `hover.primaryLanguage` | `"en"` | Preferred ordering and UI-label language; accepts `en` or `km` |
| `hover.showSource` | `true` | Show declaration path/line when available |
| `hover.showDocumentation` | `true` | Show authored/bundled prose in enabled languages |

The master switch controls this language server, not unrelated diagnostics popups or hover supplied by another server.

### Required language combinations

Use the following values within `hover`:

| Mode | `enabled` | `languages.english` | `languages.khmer` | `primaryLanguage` |
|---|---|---|---|---|
| English only | true | true | false | en |
| Khmer only | true | false | true | km |
| Both, Khmer first | true | true | true | km |
| Both, English first | true | true | true | en |
| Signature/facts only | true | false | false | en |
| Hover off | false | any | any | any |

Full bilingual example:

```json
{
  "lsp": {
    "mql-lsp": {
      "initialization_options": {
        "hover": {
          "enabled": true,
          "languages": { "english": true, "khmer": true },
          "primaryLanguage": "km"
        }
      }
    }
  }
}
```

To disable only Khmer, set `hover.languages.khmer` to `false`. To disable only English, set `hover.languages.english` to `false`. To disable all hover, set `hover.enabled` to `false`.

### Precise behavior

- Apply the same configuration to ordinary MQL and `.set` hover paths.
- Disabled languages never appear as explanation blocks, including cached AI output and fallback documentation.
- If the primary language is disabled, use the other enabled language for labels and ordering.
- If both are disabled, retain the signature and factual metadata with minimal English structural labels; omit explanatory prose and translation notices. Source code may naturally contain either language and is not rewritten.
- If a selected language has no documentation, retain facts and show a short unavailable notice in that selected language. Do not silently show prose in a disabled language or invoke AI translation.
- `showDocumentation: false` suppresses authored/bundled prose, not signatures or structural labels. Later AI prose has its own switch.
- Missing fields use defaults. Validate each present field independently; log actionable warnings for invalid values without discarding valid sibling fields or crashing. Never silently enable AI after a configuration error.
- Unknown fields are ignored for forward compatibility.
- MVP settings take effect after restarting the language server. The existing extension forwards initialization options, not a verified live-update contract. Document and manually verify the restart workflow.
- Live updates are a separate follow-up requiring verified Zed configuration notifications or configuration requests. Do not advertise immediate toggling before that is tested.

### Later configuration additions

Only document these as supported when their milestones ship:

```json
{
  "hover": {
    "showRelations": true,
    "maxRelations": 3,
    "ai": {
      "enabled": false,
      "generation": "manual",
      "usageContext": false
    }
  }
}
```

AI language output obeys the same English/Khmer switches. Generating no enabled language is a no-op. Disabling AI hides AI prose and stops new generation; disabling a language hides its cached prose without requiring cache deletion. Disabling hover stops hover-triggered background work. Manual generation and explicit cache deletion remain separate operations.

## Hover information design

### Common order

1. Symbol name/kind and signature.
2. Concise explanation in the first enabled language.
3. Explanation in the second enabled language, when enabled.
4. Direct relationships and relevant factual metadata.
5. Declaration location and provenance.

Target a short popup, typically about 10–20 logical lines for a simple bilingual symbol; wrapping varies with Zed and Khmer fonts. Limit relation lists and indicate omitted counts. Never truncate identifiers into ambiguous names.

### Information by symbol kind

| Symbol | Useful content |
|---|---|
| Function/method | Signature, purpose, parameter/return documentation, direct calls/callers, supported side-effect facts |
| Class/struct | Responsibility, base class, selected members, known instantiation sites |
| Variable/parameter/field | Type, scope, initializer or declared default, containing symbol; not a claimed runtime value |
| Macro/constant | Literal replacement/value, documented meaning, references; do not invent evaluated values |
| Enum/member | Parent enum, explicit or correctly evaluated value, documentation |
| Input | Type/default, documented units and constraints, references |
| Built-in | Curated behavior, signature, known pitfalls, official reference |
| Include | Resolved target and direct dependency; unresolved includes reported explicitly |

### Example A: authored bilingual macro documentation (MVP)

Illustrative source, not a claim about an existing EA definition:

```mql5
// @en Identifier for the recovery engine.
// @km លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។
#define ENG_REC 1
```

Illustrative bilingual hover:

```text
ENG_REC · Macro
#define ENG_REC 1

ខ្មែរ
លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។

English
Identifier for the recovery engine.

Source: declaration file and line
Documentation: source comment
```

Production labels follow the selected UI language; the example keeps metadata labels in English for clarity. Native-language review is required for the translation catalog.

English-only mode omits the Khmer heading and text. Khmer-only mode omits the English heading and text. Facts-only mode omits both prose sections. Master-off returns a null hover result.

### Example B: built-in bilingual explanation

Illustrative content for `MathAbs`, to be checked against official documentation before bundling:

```text
MathAbs · Function

English
Returns the absolute value of a number.

ខ្មែរ
ត្រឡប់តម្លៃដាច់ខាតនៃចំនួនមួយ។

Documentation: bundled reference
```

Render the actual curated signature above these summaries. Do not fabricate or substitute API signatures in the dataset.

### Example C: context-aware explanation (later AI milestone)

For the screenshot's `e == ENG_REC` branch:

- Source facts: compare `e` with the macro value; calculate `netMain`; choose `hedgeDir`; skip the current iteration if `d != hedgeDir`.
- Interpretation: nearby comments suggest recovery-trading intent.
- Unknown without further definitions: which direction index represents buy/sell and whether the overall strategy achieves its intended risk behavior.

Definition summaries are symbol-scoped. “At this usage” summaries are occurrence-scoped and include the enclosing function/branch in their cache identity. Label AI interpretation separately from parsed relationships.

## Documentation and localization model

- [ ] Add a dedicated localized documentation model rather than mixing both languages into one opaque string.
- [ ] Support `// @en ...` and `// @km ...` blocks, retaining paragraph and line boundaries.
- [ ] Define continuation semantics: untagged lines after a language tag continue that block until another tag or the declaration boundary.
- [ ] Preserve raw comments for provenance; safely render documentation as Markdown without permitting executable links or arbitrary HTML behavior.
- [ ] Classify legacy untagged comments conservatively: Khmer-script blocks as Khmer, plain English legacy docs as English, mixed/uncertain blocks as unclassified. Do not leak entire mixed blocks into a single-language mode; request explicit tags for strict filtering.
- [ ] Keep unclassified documentation available in bilingual mode with an original/unclassified label; in single-language mode use a localized unavailable/tagging notice rather than a guessed translation.
- [ ] Keep the existing built-in `doc` field backward-compatible as English and add an optional `doc_km` field. Audit documentation consumers so completion behavior does not accidentally change.
- [ ] Add localized labels and missing-translation messages in one catalog.
- [ ] Establish a reviewed glossary: parameter, return value, dependency, caller, lot, point, tick, hedge, and drawdown. Keep technical English terms alongside Khmer where clearer.
- [ ] Require review for trading units, negations, and risk-related translations. Do not auto-translate identifiers.

## Architecture

```text
Workspace files + synchronized unsaved buffers + standard library
                         |
                  Parser / resolver
                         |
            Symbol and direct-relationship index
                         |
        Facts + localized documentation + valid AI cache
                         |
           Settings filter and Markdown renderer
                         |
                  LSP hover response
```

AI generation is an optional separate worker consuming bounded evidence and writing validated cache entries. It must not block the LSP request loop. No graph database or vector database is required for the initial implementation.

### Proposed module boundaries

- `lsp/src/hover.rs`: dedicated renderer and hover composition.
- `lsp/src/hover_config.rs`: typed settings, defaults, validation, and language selection.
- `lsp/src/localization.rs`: labels and localized documentation selection.
- `lsp/src/index.rs`: initially retain indexing; evolve toward structured symbols through later milestones.
- `lsp/src/main.rs`: route requests, apply master switch to both MQL and `.set`, pass settings to renderer.
- Parser/resolver/relationship modules: choose concrete layout after the parser evaluation, not in advance.
- Shared analysis library for LSP and MCP: extract only after the semantic model stabilizes.

## Milestones

### H1 — Configurable bilingual factual hover (MVP)

This milestone improves existing recognized symbols; it does not promise complete semantic resolution, call graphs, or automatic translation of arbitrary code.

Tasks:

- [ ] Implement and test the MVP configuration contract.
- [ ] Extract hover rendering from `main.rs`; preserve existing completion documentation behavior.
- [ ] Apply master disable and language filters to every hover path, including `.set`.
- [ ] Show kind and declaration source where known; retain ambiguous candidates with explicit provenance instead of claiming a unique target.
- [ ] Preserve structured bilingual source comments and paragraphs.
- [ ] Add curated Khmer documentation for a small prioritized built-in set; track missing translations explicitly.
- [ ] Correct UTF-16 position conversion and return a precise identifier range.
- [ ] Add configuration examples and restart instructions to README/GUIDE when implementation ships.
- [ ] Verify Khmer rendering in a real Zed hover on macOS.

Acceptance:

- [ ] All six configuration modes in the table behave as specified.
- [ ] Existing projects with no hover configuration retain English hover behavior.
- [ ] No network calls or model credentials are required.
- [ ] Turning hover off leaves completion, definitions, and diagnostics working.
- [ ] Disabled-language text never appears through documentation fallback or cached prose.
- [ ] Unsupported translations are not replaced with invented explanations.

### H2 — Broader coverage and accurate resolution

- [ ] Evaluate an MQL-compatible syntax-tree parser against representative `.mq5` and `.mqh` fixtures. A C++ grammar is only a candidate, not proof of MQL compatibility.
- [ ] Introduce symbol identity using file, container/scope, kind, name, and signature; do not use bare names as unique IDs.
- [ ] Model locals, globals, parameters, fields, methods, constructors, structs, and enum members distinctly.
- [ ] Handle multiline declarations, shadowing, member access, inheritance, and basic overload resolution.
- [ ] Resolve includes in the correct compilation context; do not mix unrelated EA programs merely because they share a workspace.
- [ ] Account for preprocessing/conditional branches; label unsupported expansion and unresolved external imports rather than pretending to compile the language fully.
- [ ] Keep synchronized unsaved buffers authoritative and handle changed/deleted files.

Acceptance: same-named methods in different classes and shadowed variables resolve correctly in supported cases; ambiguous overloads are explicitly unresolved; no wrong-symbol explanation is attached as certain.

### H3 — Direct relationships

- [ ] Index direct references, calls/callers, base types, members, and includes.
- [ ] Add conservative read/write classification where supported; distinguish direct effects from inferred transitive effects.
- [ ] Track unresolved edges and index coverage. “No callers found” must not mean “never called,” especially for MQL event handlers invoked by the platform.
- [ ] Add bounded relationship summaries and full reference navigation.
- [ ] Expose standard LSP references/call hierarchy/type hierarchy where implemented and supported by the client; verify Zed UI rather than assuming every protocol feature is surfaced.

Acceptance: each displayed relationship has a source location; text matches in comments/strings are not reported as semantic callers; recursive calls and partial indexes are handled without loops or misleading completeness claims.

### H4 — Optional AI explanations and translation

- [ ] Implement explicit generation before opt-in background precomputation. Never issue an API call for every mouse movement.
- [ ] Verify the chosen entry point: LSP code action/execute command, a Zed task, or MCP. Do not assume custom hover buttons or access to Zed's configured model credentials.
- [ ] Package resolved source, comments, bounded dependencies, provenance, and unknowns as evidence.
- [ ] Produce structured English/Khmer summaries with separate source-fact and interpretation fields.
- [ ] Validate generated output and references; source comments are untrusted data, not instructions to the model.
- [ ] Keep AI off by default. Require explicit local/cloud provider setup, source-sharing consent, protected credentials, request budgets, and concurrency/time limits.
- [ ] Do not treat project-controlled provider endpoints or settings as authorization to upload source code.
- [ ] Cache by workspace/compilation context, symbol identity, source/context/dependency fingerprints, language, model, and prompt/schema version.
- [ ] Account for unsaved versions; cancel or discard obsolete generation results. Hide stale explanations by default and preserve current facts.
- [ ] Provide cache deletion, local storage boundaries, and a documented retention policy; do not commit generated private-source caches.
- [ ] Keep slow inference off the LSP loop and respect request cancellation.

Acceptance: local hover remains usable offline and during model failure; repeated hovers make no repeated paid calls; edits invalidate affected explanations; disabled AI/languages do not render cached AI prose.

Standard LSP hover returns one result, with no standard streaming/refresh mechanism for an already-open tooltip. Serve cached content and show new results on the next request. Any richer refresh behavior requires a separately verified Zed integration.

### H5 — Deeper understanding and agent reuse

- [ ] Provide a generated Markdown explanation or Agent workflow for long walkthroughs, examples, and larger relationship diagrams.
- [ ] Reuse the analysis model through proposed MCP tools such as `describe_symbol`, `get_symbol_dependencies`, `get_symbol_callers`, and `get_change_impact`.
- [ ] Report change impact as direct evidence plus bounded inference, not an exhaustive safety guarantee.
- [ ] Define how MCP receives a current snapshot; do not claim it sees unsaved LSP buffers unless a bridge exists. Report snapshot identity and disk/unsaved provenance.
- [ ] Treat custom panels, tabs, graph widgets, and executable hover links as optional compatibility investigations, not MVP requirements.

## Validation plan

### Automated

- [ ] Settings: defaults, partial objects, each toggle combination, disabled primary language, invalid field types, unknown fields, preservation of sibling initialization options.
- [ ] Renderer snapshots: English-only, Khmer-only, bilingual orderings, facts-only, master-off, missing translations, unclassified mixed comments, `.set` hover, and absent declaration locations.
- [ ] Unicode: Khmer combining sequences, accented text, emoji before identifiers, CRLF, end-of-line positions, precise ranges.
- [ ] Parser/resolver fixtures: duplicate names, shadowing, overloads, multiline signatures, inheritance, macros, includes, inactive/unknown preprocessor branches, comments/strings, malformed code while typing.
- [ ] Protocol tests: initialize with settings, didOpen/didChange, hover response/null, `.set` path, no regressions to completion/definition, configuration applied after server restart.
- [ ] AI worker: network denial, timeout, cancellation, cost/concurrency limits, prompt injection in comments, cache language isolation, stale dependency and unsaved-edit invalidation.

### Manual Zed acceptance

- [ ] Demonstrate the six setting modes using the same source fixture.
- [ ] Confirm initialization options arrive after the documented restart operation.
- [ ] Check Khmer font fallback, shaping, wrapping, clipping, and mixed-script code blocks.
- [ ] Verify standard-library, workspace, local-symbol (H2), and `.set` hovers.
- [ ] Verify source navigation/link behavior before promising clickable paths.
- [ ] Test hover after AI edits an open file and after an included file changes.
- [ ] Check that no unwanted source upload occurs in default configuration.

Performance target, not an existing measurement: warmed local/cached hover p95 under 100 ms server processing time on a recorded representative workspace, excluding Zed's intentional hover delay. Record machine, corpus size, indexing state, and memory consumption. Indexing and AI must not stall document synchronization.

## Delivery boundaries and risks

- H1 is the first shippable release; full local variable/member coverage starts in H2.
- Khmer output is easy to transport as Unicode, but translation quality and rendering require real review.
- Regex improvements alone will not deliver reliable scope/type/call resolution.
- Static analysis cannot generally know runtime values, dynamic dispatch targets, or every side effect. Report analysis boundaries.
- Language settings are initialization-time in the MVP; live changes remain unverified.
- Existing grammar configuration in Zed does not automatically expose an AST to the standalone LSP.
- An MCP explanation does not automatically change editor hover, and hover text does not automatically become agent context. Shared data must be explicitly integrated.
- No new cloud service, model dependency, custom panel, graph database, or automatic generation is required for H1.

## References

- Zed language extensions: https://zed.dev/docs/extensions/languages
- LSP hover: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_hover
- LSP position encodings: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position
- LSP call hierarchy: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_prepareCallHierarchy

## Next implementation step

Implement H1 only: configuration, localized structured docs, common hover rendering, precise ranges, targeted tests, and a real Zed bilingual fixture. Leave semantic relationships and AI generation out until the factual/configuration foundation is verified.
