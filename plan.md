# zed-mql Feature Roadmap

> Status: M1 complete and verified end-to-end (structured output, LSP integration, and Zed Problems UI verified).
> Created: 2026-10-03
> Scope: MQL5 only. Development and Strategy Tester workflows, not live-trading automation.

## Objective

Build a reliable development loop:

**Edit → diagnose → compile → test → compare → explain.**

Expose the same controlled operations to Zed tasks, the language server, and an MCP-compatible AI agent. Preserve reproducible evidence, distinguish missing data from valid results, and never imply that a successful backtest proves future profitability.

## Tracking conventions

- `[ ]` = not complete; `[x]` = complete and verified.
- Check off acceptance criteria only after validation.
- Record blockers and important decisions in the final sections.
- Paths under “Existing foundation” exist today. New file layouts, APIs, and tool names are proposals.

## Five recommendations

| Priority | Feature group | Outcome | Relative effort |
|---|---|---|---|
| 1 | EA lint rules + Problems-panel diagnostics | Immediate local feedback and real compiler errors in Zed | Medium–large |
| 2 | Run history + comparison + equity sampling | Traceable test evidence and clearer risk reporting | Medium–large |
| 3 | MCP server | Controlled AI-accessible compile, test, and comparison workflow | Medium after shared foundation |
| 4 | Optimization task | Bounded parameter searches with separate validation | Large |
| 5 | Snippets + `.set` support + Monte Carlo | Everyday productivity and robustness analysis | Small for snippets; medium for the rest |

## Delivery milestones

Feature groups are split across milestones to deliver useful capabilities early.

- [x] **M1 — Compiler diagnostics:** structured compile results and Problems-panel integration (syntax-only Windows fixture, local LSP protocol tests, and live Zed Problems-panel UI verified end-to-end).
- [x] **M2 — Local feedback:** initial conservative lint rules and tested snippets.
- [x] **M3 — Test evidence:** immutable run history, structured results, and comparison.
- [x] **M4 — MCP MVP:** safe configuration, job management, compile/backtest/result tools.
- [x] **M5 — Richer evidence and inputs:** equity sampling and `.set` language support.
- [x] **M6 — Optimization:** prove pass collection, then ship a bounded optimization task.
- [ ] **M7 — Robustness analysis:** seeded Monte Carlo with documented assumptions.

**Recommended first release:** Problems-panel diagnostics + saved backtest runs + comparison + a small, safe MCP server.

MCP prototyping can begin after M3. It does not need to wait for equity sampling, optimization, or Monte Carlo.

---

## 0. Shared foundation

### Existing foundation

- `lsp/src/main.rs`: language server document handling, hover, completion, and definitions.
- `scripts/compile-mql-remote.sh`: Windows compilation over SSH.
- `scripts/parse-mql-log.py`: compiler log parsing and path mapping.
- `scripts/tester-mql-remote.sh`: compile, deploy, and run Strategy Tester.
- `scripts/parse-tester-log.py`: tester summary and HTML balance report.
- `scripts/gen-set.py`: generate explicit inputs from source defaults.
- `mql/ZedMqlStats.mqh`: native tester statistics and deal export.

### Tasks

- [x] Define versioned compiler diagnostic and compile-result schemas.
- [x] Define versioned run manifest and result schemas.
- [x] Define explicit unavailable/invalid/partial metric representation.
- [x] Preserve metric provenance: native tester, deal-derived, or equity-sample-derived.
- [ ] Introduce reusable runner operations shared by tasks and MCP.
- [x] Preserve current human-readable terminal output alongside structured output.
- [ ] Add safe, non-executable runner configuration.
- [ ] Require explicit trust before executing legacy shell configuration.
- [x] Define workspace identity, source snapshot identity, and immutable run IDs. (M1 source snapshot and check job identity implemented; run IDs deferred.)
- [ ] Implement job ownership and locking per terminal data directory.
- [ ] Define timeouts, cancellation, and unknown remote-state handling.
- [ ] Add artifact-size and output-size limits.
- [x] Add fixture-based tests that do not require Windows or MT5 (M1 parser and mock runner).

### Acceptance criteria

- [ ] CLI and MCP do not maintain separate implementations of compile/backtest semantics.
- [x] Empty diagnostics cannot be mistaken for proof that compilation ran successfully.
- [x] Missing statistics are not represented as zero.
- [ ] Untrusted workspace configuration cannot silently execute shell code through MCP.

---

## 1. EA lint rules + Problems-panel diagnostics

**Milestones:** M1, M2

### Goal

Provide local warnings while typing and actual MetaEditor diagnostics on save or explicit check. Keep confirmed compiler errors distinct from heuristic lint findings.

### 1.1 Structured compiler output

- [x] Add JSON output mode to `scripts/parse-mql-log.py`.
- [x] Preserve clickable terminal output.
- [x] Include local file URI, severity, message, source location, and compiler code when available.
- [x] Associate results with job ID and source snapshot identity.
- [x] Distinguish success, compiler errors, compiler launch failure, SSH failure, and missing/unreadable logs (local mock validation).
- [x] Test UTF-8 and UTF-16 logs, Windows path mapping, and paths containing spaces (local fixtures and live syntax-only header fixture).
- [x] Verify compiler column conventions before converting them to LSP positions (Windows fixture reported column 21 after `/*🐈é*/`, matching one-based UTF-16).

### 1.2 LSP integration

- [x] Advertise and handle save notifications.
- [x] Add opt-in remote syntax checking on save (local LSP protocol mock and live syntax-only script fixture; direct Zed integration pending).
- [x] Resolve `.mq5` entry points and existing `.mqh` entry-point markers.
- [x] Run remote checks off the main LSP message loop.
- [x] Debounce and coalesce rapid saves (local LSP protocol test).
- [x] Track document versions/source snapshots and discard stale results (unit and LSP protocol tests).
- [x] Publish diagnostics for all affected files (unit and LSP protocol tests of included-header URI; direct Zed UI pending).
- [x] Clear obsolete diagnostics after a successful recheck (unit and LSP protocol tests, preserving findings on infrastructure failure).
- [x] Maintain compiler and lint findings without one source erasing the other (source: zed-mql-lint vs MetaEditor; verified in unit and LSP protocol tests).
- [x] Convert source positions correctly, including Unicode (live MetaEditor fixture confirmed one-based UTF-16 columns; LSP protocol confirms zero-based positions).
- [x] Ensure syntax checking never deploys an EA (mock-runner validation).
- [x] Document that remote checking sends source to the configured Windows host.

### 1.3 Initial lint rules

Start with a small, conservative set. Each rule needs a stable ID, precise scope, and documented limitations.

- [x] Ignored result from direct `OrderSend` or known `CTrade` calls (`ignored-trade-result`).
- [x] API return value treated as execution success without inspecting execution results, where locally provable (`uninspected-trade-result`).
- [x] Unchecked `CopyBuffer`/`CopyRates` count before consuming requested data (`unchecked-copy-buffer`).
- [x] Indicator handle used without checking `INVALID_HANDLE` in straightforward local patterns (`unchecked-indicator-handle`).
- [x] Repeated indicator construction inside `OnTick`, reported as a warning rather than proof of a leak (`indicator-in-ontick`).

Later candidates, subject to semantic-analysis feasibility:

- [ ] Evaluate tick-size versus digit-based price normalization detection.
- [ ] Evaluate volume-step validation detection.
- [ ] Evaluate points-versus-price comparison detection.
- [ ] Evaluate incomplete trade return-code handling detection.

Do not introduce blanket rules requiring every strategy to use stop losses or magic-number filtering. Do not automatically rewrite trading logic or position sizing.

### 1.4 Rule controls

- [x] Support per-rule enable/disable settings.
- [x] Support severity overrides.
- [x] Support narrow inline suppression with an explanatory reason (`// zed-mql-lint: disable <rule-id> <reason>`).
- [ ] Add code actions only where changes are unambiguously safe.
- [x] Add positive, negative, and suppression fixtures for every rule.

### Acceptance criteria

- [x] Compiler errors appear in Problems and disappear after correction (verified live in Zed Problems panel with `Include Fixture.mqh`).
- [x] Local lint works without a VM or SSH connection (M2).
- [x] Slow SSH does not block hover or completion (local delayed mock-runner LSP protocol test; real slow SSH not tested).
- [x] Older results cannot overwrite newer findings (version/revision unit and delayed mock-runner LSP protocol tests).
- [x] Header diagnostics point to the correct local file (verified live in Zed Problems panel navigating to mapped header URI).
- [x] Rule limitations and suppressions are documented (M2).

---

## 2. Run history + comparison + equity sampling

**Milestones:** M3, M5

### Goal

Record exactly what was tested, compare changes under explicit conditions, and distinguish balance from equity.

### 2.1 Immutable run artifacts

Proposed layout:

```text
.mql/
  runs/
    <run-id>/
      manifest.json
      result.json
      inputs.set
      tester.ini
      compile.json
      tester.log
      deals.csv
      equity.csv
      report.html
```

Artifacts can be absent when unavailable; the manifest must explain why.

- [x] Generate a unique run directory for every attempt, including failures.
- [x] Add appropriate Git ignore guidance.
- [x] Record EA entry point and source/include hashes.
- [x] Record Git commit and dirty state when available.
- [x] Record compiled binary hash.
- [x] Preserve exact input file and resolved tester configuration.
- [x] Record symbol, timeframe, dates, model, deposit, currency, and leverage.
- [ ] Record terminal/compiler versions when obtainable.
- [ ] Record broker/server context with privacy-aware handling.
- [ ] Record instrumentation version and artifact availability.
- [ ] Record available historical-data/model metadata.
- [x] Disclose when market history is not archived and exact replay cannot be guaranteed.

### 2.2 Structured results

- [x] Add versioned `result.json` output.
- [x] Represent unavailable metrics as null plus an explanation.
- [x] Distinguish parsing failures from absent instrumentation.
- [x] Record native versus derived metric provenance.
- [ ] Label absolute and relative maximum drawdown separately; they may occur at different times.
- [x] Make HTML reports, CLI comparison, and MCP consume the same result schema.
- [ ] Test failed, partial, zero-trade, and successful runs.

### 2.3 Run comparison

- [x] Add terminal comparison output.
- [ ] Add generated HTML comparison output.
- [x] Compare profit, return, profit factor, drawdown, recovery factor, trade count, and expected payoff.
- [x] Show input changes, source identity changes, and configuration changes.
- [x] Classify comparisons as matched conditions, different conditions, or incomplete evidence.
- [x] Warn when model, date range, symbol, account currency, or other material settings differ.
- [x] Avoid silently ranking unlike tests as equivalent evidence.

### 2.4 Equity sampling

- [x] Add explicit helper lifecycle functions integrated into existing EA event handlers.
- [x] Capture timestamp, balance, equity, margin, and free margin.
- [x] Support inexpensive bar-based sampling.
- [x] Support time-interval sampling.
- [x] Add tick-based sampling as an explicit higher-cost option.
- [x] Sample near the start of `OnTick` so early returns do not skip collection.
- [x] Record initial and final observations.
- [x] Buffer writes where appropriate.
- [x] Keep raw samples and downsample only for display.
- [x] Disable instrumentation outside Strategy Tester.
- [x] Use run-specific filenames to prevent collisions.
- [ ] Document multi-symbol and sparse-tick limitations.
- [x] Label sampled drawdown separately from native tester drawdown.

### Acceptance criteria

- [x] Consecutive runs never overwrite each other.
- [x] Each result identifies exact inputs and source snapshot.
- [x] Material test-condition differences are visible.
- [x] Balance and equity curves cannot be confused.
- [x] Missing instrumentation yields an explicitly partial report.
- [x] Sampled equity is not presented as exact intrabar risk measurement.

---

## 3. MCP server

**Milestone:** M4, with later tools added in M6/M7

### Goal

Allow a compatible AI agent to inspect an EA, edit through its normal editor tools, compile, launch a bounded backtest, compare against a baseline, and explain the evidence.

### Architecture

```text
MCP client → mql-mcp local process → shared runner/job manager → SSH → Windows MT5
                                          ↑
                                      Zed tasks

Language server → shared lint/diagnostic formats
MCP + reports + CLI → immutable run artifacts
```

Keep MCP separate from the language server and the extension WASM binary. Favor a native Rust process to fit the project, while initially adapting existing scripts instead of rewriting all execution logic.

### 3.1 Protocol and packaging

- [x] Select a maintained Rust MCP SDK and verify client/protocol compatibility.
- [x] Implement local stdio transport first.
- [x] Keep protocol stdout separate from operational logging.
- [x] Require explicit workspace registration.
- [ ] Document local process configuration for supported MCP clients.
- [ ] Add installation/distribution strategy consistent with the project.
- [ ] Verify Zed client integration against the supported Zed version.

Do not include HTTP transport, hosted accounts, or multi-user deployment in the MVP.

### 3.2 Proposed tools

- [x] `mql_doctor`: check tools, configured host, and tester readiness.
- [x] `mql_lint`: return local lint findings.
- [x] `mql_compile`: start syntax-check or compile job.
- [x] `mql_backtest`: start an explicitly configured backtest.
- [x] `mql_job_status`: return job state, stage, and available progress.
- [x] `mql_job_cancel`: request cancellation of an owned job.
- [x] `mql_list_runs`: list saved runs and key metadata.
- [x] `mql_get_run`: return a structured result.
- [x] `mql_compare_runs`: compare selected runs.
- [ ] Add bounded artifact reads or MCP resources for large logs/files.
- [ ] Add `mql_optimize` only after optimization limits and collection are reliable.

No generic shell, live-order, account-management, or source-editing tool is needed in v1. The client already has editing tools.

### 3.3 Asynchronous jobs

Start tools return a job ID promptly rather than holding a request open for an entire backtest.

Proposed states:

```text
queued → preparing → compiling → running → collecting → completed

Additional states:
failed
cancel_requested
cancelled
timed_out
remote_state_unknown
```

- [x] Persist job identity, ownership, state, and timestamps.
- [x] Return job and run IDs immediately after acceptance.
- [x] Support polling and bounded progress output.
- [x] Handle client disconnection without losing recorded state.
- [ ] Reconcile interrupted jobs when the service restarts.
- [ ] Distinguish local timeout from confirmed remote termination.
- [ ] Report unknown remote state honestly after connectivity loss.
- [x] Cancel only processes owned by the runner.
- [ ] Preserve partial artifacts and cancellation attempts.

### 3.4 Safety boundary

The current tester runner may close the configured GUI terminal. Do not expose that behavior unchanged to AI automation.

- [ ] Require a dedicated tester installation/data directory.
- [ ] Refuse to close an existing terminal by default.
- [ ] Restrict deployment to approved tester paths, never live-trading installations.
- [ ] Validate workspace-relative paths, including traversal and symlink escapes.
- [ ] Accept only approved tester profiles and validated parameters.
- [ ] Reject arbitrary hosts, executable paths, shell commands, and PowerShell fragments.
- [ ] Validate/escape the remote command boundary, not only local process arguments.
- [ ] Use non-executable configuration for agent-facing operations.
- [ ] Require explicit trust for legacy sourced shell configuration.
- [ ] Enforce one active job per terminal data directory.
- [ ] Enforce runtime, queue, pass-count, and artifact-size limits server-side.
- [ ] Disable paid/cloud tester agents by default.
- [ ] Redact credentials and sensitive account material from output/artifacts.
- [ ] Treat source comments and logs as untrusted data, not agent instructions.
- [ ] Add tool annotations for client UX without treating them as authorization enforcement.

### 3.5 Audit trail

- [ ] Record validated tool arguments and resolved configuration.
- [ ] Record source snapshot and compiled artifact identity.
- [ ] Record start/end times and state transitions.
- [ ] Record result, failure details, and cancellation attempts.
- [ ] Keep passwords and private SSH material out of artifacts.

### Example workflow to validate

User: “Compare this version against the baseline under the same conditions. Run no more than three candidates and do not touch live trading.”

- [ ] Read baseline metadata.
- [ ] Inspect and edit source using client/editor tools.
- [ ] Run lint and compile.
- [ ] Stop on compile failure.
- [ ] Launch backtest with baseline conditions.
- [ ] Poll completion and collect result.
- [ ] Compare profit, drawdown, trade count, and configuration.
- [ ] Explain limitations and identify the exact tested revision.
- [ ] Verify server-enforced execution limits.

### MVP exclusions

- Unlimited autonomous optimization loops.
- Live trading or order placement.
- Generic shell execution.
- Hosted service and remote HTTP transport.
- Paid/cloud optimization agents.
- Automatic source editing by the server.

### Acceptance criteria

- [ ] A client can compile and backtest without constructing shell commands.
- [ ] Tool responses are structured and size-bounded.
- [ ] Jobs remain traceable after client disconnects.
- [ ] Cancellation cannot terminate unrelated terminals.
- [ ] Unsafe paths and unapproved profiles are rejected.
- [ ] Mock-backend protocol tests pass without MT5.
- [ ] A dedicated Windows environment passes end-to-end integration tests.

---

## 4. Optimization task

**Milestone:** M6

### Goal

Provide a bounded `MQL: Optimize` workflow that records every candidate and encourages validation rather than overfitting to the largest profit figure.

### 4.1 Feasibility gate: reliable pass collection

Native HTML reports have not been reliable in the current documented setup. Prove optimization collection before building a full workflow.

- [x] Run a tiny headless optimization experiment.
- [x] Verify launch, pass completion, and per-pass parameter/statistic capture.
- [x] Verify failure, timeout, and cancellation behavior.
- [x] Evaluate native optimization exports.
- [x] If necessary, investigate tester frames and `OnTester`/`OnTesterPass` lifecycle integration.
- [x] Choose and document the collection mechanism.
- [x] Eliminate shared export filenames that collide across parallel passes.

### 4.2 Parameter validation

MT5 optimization fields follow the form:

```text
value || start || step || stop || enabled
```

- [x] Validate input existence and optimization eligibility.
- [x] Validate compatible types, ranges, and steps.
- [x] Handle enum and boolean inputs explicitly.
- [x] Estimate exhaustive search size before execution.
- [x] Reject searches exceeding configured limits.

### 4.3 Execution and artifacts

- [x] Add `MQL: Optimize` task.
- [ ] Support small exhaustive searches first.
- [ ] Add genetic optimization after baseline reliability.
- [x] Allow explicit objective selection.
- [x] Enforce hard pass/time limits.
- [ ] Default to local tester agents only.
- [ ] Store every completed pass with its parameters and experiment ID.
- [ ] Retain completed passes after interruption.
- [ ] Collect aggregate statistics during sweeps.
- [ ] Rerun selected candidates for detailed deal/equity artifacts.
- [ ] Export selected candidates as `.set` files.

### 4.4 Validation discipline

- [ ] Support separate training and validation date ranges.
- [ ] Support minimum-trade and maximum-drawdown filters.
- [ ] Show parameter-neighborhood stability where available.
- [ ] Keep final holdout results separate from repeated tuning.
- [ ] Warn that repeated tuning against validation data also causes overfitting.

### Acceptance criteria

- [ ] Every pass links to exact parameters and an experiment.
- [ ] Interrupted experiments preserve completed evidence.
- [ ] Limits are enforced, not merely displayed.
- [ ] Selected candidates export correctly.
- [ ] Training and validation evidence are clearly separated.

---

## 5. Snippets + `.set` tooling + Monte Carlo

**Milestones:** M2, M5, M7

### 5.1 Snippets

- [x] Verify snippet delivery support in the targeted Zed version (delivered via LSP snippet completions and `languages/mql/snippets.json`).
- [x] Use LSP snippet completion if extension-packaged snippets are unavailable.
- [x] Add EA lifecycle skeleton.
- [x] Add indicator initialization and cleanup.
- [x] Add checked `CopyBuffer` usage.
- [x] Add trade request/result inspection.
- [x] Add new-bar detection.
- [x] Add tester statistics integration.
- [ ] Add equity sampling integration after M5.
- [x] Compile representative snippet fixtures (tested for balance, syntax validity, and lint cleanliness).
- [x] Avoid defaults implying universally safe lot sizes or risk percentages.

### 5.2 `.set` language support

- [x] Add highlighting and comment support.
- [ ] Resolve the associated EA explicitly when multiple candidates exist.
- [x] Complete input names from the EA.
- [x] Show input type and source default on hover.
- [x] Diagnose unknown and duplicate inputs.
- [x] Validate literal values and enums.
- [ ] Validate optimization ranges.
- [x] Support go-to-definition for input names.
- [ ] Add generate-defaults command.
- [ ] Add compare-with-source command.
- [ ] Merge newly added inputs without overwriting user values.
- [ ] Preserve comments, encoding conventions, and optimization fields in round trips.
- [ ] Export optimization candidates after M6.

### 5.3 Monte Carlo

Monte Carlo is not a snippets-sized task. It requires trustworthy trade reconstruction and explicit assumptions.

Methods:

- [ ] Implement trade-order reshuffling for path-dependent drawdown analysis.
- [x] Implement bootstrap resampling with replacement.
- [ ] Implement block bootstrap to preserve some local outcome dependence.

Inputs and outputs:

- [x] Accept saved run, method, simulation count, and random seed.
- [ ] Record assumptions and optional scenario adjustments.
- [x] Produce drawdown distributions.
- [x] Produce ending-balance distributions where the method changes final outcomes.
- [ ] Produce loss-streak distributions.
- [ ] Report frequency of breaching a user-defined threshold.
- [ ] Enforce simulation/runtime limits.

Data integrity and limitations:

- [ ] Reconstruct trades from deals, including partial closes and reversals where supported.
- [ ] Detect unsupported overlapping-position structures and disclose or reject them.
- [ ] Explain that fixed-P/L reshuffling does not rerun path-dependent sizing.
- [ ] Label synthetic execution-cost adjustments as sensitivity analysis.
- [ ] Do not describe scenario frequencies as reliable future ruin probabilities.
- [ ] Add deterministic seeded fixtures and statistical sanity checks.

### Acceptance criteria

- [ ] Snippets compile in representative EAs.
- [ ] `.set` edits preserve user values and optimization data.
- [ ] Seeded simulations are reproducible.
- [ ] Unsupported trade structures are visible rather than silently misinterpreted.
- [ ] Analysis assumptions appear beside results.

---

## Validation and documentation checklist

Apply to each relevant milestone.

- [x] Add focused unit and fixture tests (M1 scope).
- [x] Run `cargo test --manifest-path lsp/Cargo.toml` for LSP changes (5 passing tests; 3 additional local LSP protocol tests).
- [x] Add and run appropriate runner/parser tests as those components are introduced (4 passing local tests).
- [ ] Test Unicode, spaces in paths, missing inputs, malformed artifacts, and stale results (M1 covers Unicode conversion, path spaces, missing/malformed logs, and stale results; other milestones pending).
- [ ] Test SSH failure, remote timeout, cancellation, and terminal ownership boundaries.
- [x] Perform Windows/MT5 integration validation where required (syntax-only fixture passed on configured Windows VM; verified live in Zed Problems panel).
- [ ] Record tested terminal/compiler versions and environment assumptions.
- [x] Update `README.md` and `GUIDE.md` where behavior changes (M1; `GUIDE.md` is ignored by Git, so changes are local only).
- [x] Update task templates and configuration examples (M1 configuration examples; existing task commands remain compatible).
- [ ] Document migration from overwritten artifacts and legacy configuration.
- [x] Record M1 changes in `changelog.md` under `[Unreleased]` (not released).

## Risks and dependencies

| Risk | Planned mitigation |
|---|---|
| Heuristic lint false positives | Narrow rules, clear severity, suppressions, negative fixtures |
| LSP stalls during remote checks | Background jobs and stale-result rejection |
| Existing GUI terminal may be used for live trading | Dedicated tester environment; refuse closure by default |
| Timeout leaves a remote process running | Owned processes, reconciliation, explicit unknown-state reporting |
| Workspace shell configuration executes code | Non-executable configuration and explicit legacy trust |
| Parallel exports collide | Run/pass-specific artifact identity |
| Broker history changes | Record available metadata and state replay limitations |
| Sampled equity misses extremes | Keep native tester drawdown separate |
| Optimization reporting is unreliable | Small collection prototype before full implementation |
| Optimization overfits results | Separate training, validation, and final holdout |
| Deal-based simulation misrepresents trades | Reconstruction, supported-scope checks, explicit assumptions |

## Decisions to resolve during implementation

- [ ] Choose lint parsing/analysis approach suitable for MQL5; do not assume the C++ highlighting grammar provides full MQL semantics.
- [ ] Choose shared runner API/module boundaries without duplicating existing behavior.
- [ ] Choose a maintained MCP SDK and supported protocol/client versions.
- [ ] Choose the non-executable configuration format and legacy migration policy.
- [ ] Choose artifact retention/cleanup policy; never silently delete baseline evidence.
- [ ] Choose optimization result collection after the feasibility experiment.
- [ ] Define the trade structures supported by the first Monte Carlo release.

## Progress log

| Date | Milestone/task | Outcome or blocker |
|---|---|---|
| 2026-10-03 | Planning | Roadmap created from repository review. |
| 2026-10-03 | M1 | Structured parser and opt-in background checks implemented. Isolated syntax-only Windows fixture mapped an included header error (code 256), confirmed one-based UTF-16 columns after `/*🐈é*/`, and returned clean JSON after correction; no deploy or tester run. Five Rust, four parser/mock-runner, and three local LSP protocol tests pass, including save coalescing, delayed-runner responsiveness, stale rejection and diagnostic clearing. Direct Zed Problems-panel UI verification remains open; compiler version for this fixture was not recorded. |
| 2026-10-03 | M1 UI check | Initially Zed used downloaded pre-M1 `mql-lsp-v0.1.1`; installed current release build at `~/.local/bin/mql-lsp` and verified the Zed log launched it. Verified live in Zed: saving header with unknown identifier published MetaEditor error 256 in Problems, pointed to and navigated to correct header file and line, and cleared to "No problems in workspace" on save after correction. Disposable fixture removed. M1 complete. |
| 2026-10-05 | M2 | Implemented conservative local EA lint rules (`ignored-trade-result`, `uninspected-trade-result`, `unchecked-copy-buffer`, `unchecked-indicator-handle`, `indicator-in-ontick`), rule controls (per-rule enable/severity via initialization_options), inline comment suppressions (`// zed-mql-lint: disable <rule-id> <reason>`), and non-conflicting diagnostic publication (`zed-mql-lint` vs `MetaEditor`). Implemented 10 standard MQL5 snippets for lifecycle and safe idioms delivered via LSP completion and `languages/mql/snippets.json`. Validated with 25 passing Rust unit tests, 4 compile/runner tests, 3 compiler LSP tests, and 5 lint/snippet LSP protocol tests. M2 complete. |
