# Continuous EA Research Agent — Implementation Plan

**Status:** implementation plan only, based on a read-only source audit on 2026-10-08. No remote backtest or runtime verification was performed for this audit. Historical completion claims in the separate extension roadmap are not proof that the current autonomous workflow is complete. The source findings below must be resolved before unattended operation.

## Goal and scope

Build a persistent research controller that generates approved EA settings, runs backtests, records every attempt, validates candidates, and chooses the next bounded batch until the user pauses or stops the campaign. Reuse the existing extension, scripts, and MCP interface; do not automate live deployment or promise profitability.

| Area | Requirement |
|---|---|
| EA | `EA-Research-Lab/FlexUltimateGRH/FlexUltimateGRH.mq5`; freeze source and includes per campaign |
| Brokers | Lirunex first, Exness added through worker profiles |
| Markets | Configurable broker-supported symbols; gold and Bitcoin are examples, not a fixed list |
| Strategies | Independent campaigns for selected entry modes, with strategy-specific parameter ranges |
| Intended trading accounts | Separate account per symbol/strategy combination |
| Cent starting capital | 10,000 account cents, approximately USD 100 when the denomination is USD cents |
| Standard starting capital | USD 1,000; standard-account campaigns remain disabled until their risk ceiling is approved |
| Cent drawdown ceiling | Reject results exceeding 50% maximum peak-to-trough equity drawdown; prefer lower drawdown |
| Existing infrastructure | One Lirunex MT5 installation used only for backtesting; Exness not installed yet |
| Live boundary | Future live terminal must have separate installation/data paths and remain inaccessible to research operations |

Peak-relative equity drawdown and loss relative to initial deposit must be reported separately. A 50% drawdown from a later equity peak is not necessarily 5,000 cents. Screening thresholds are not guaranteed live-loss caps.

## 1. Source audit findings

| Finding | Evidence | Required action |
|---|---|---|
| Backtest is simulated; doctor/lint return placeholder responses | `mql-mcp/src/main.rs` | Connect real execution/checks and remove fabricated success responses |
| Jobs live only in a memory map | `mql-mcp/src/jobs.rs` | Persist jobs and reconcile them after restart |
| Failure branches call `complete_job`, which changes state to `Completed` | `mql-mcp/src/main.rs`, `mql-mcp/src/jobs.rs` | Separate terminal state from result storage |
| Cancellation only sets `CancelRequested` | `mql-mcp/src/main.rs` | Cancel owned processes and confirm remote termination |
| Run listing/retrieval/comparison are placeholders | `mql-mcp/src/main.rs` | Connect existing artifacts and comparison logic |
| Optimization copies XML but finishes with unconditional success; structured parser invocation is commented out | `scripts/optimize-mql-remote.sh` | Validate report freshness, parse passes, and classify failures |
| Shared remote filenames/tasks and second-resolution run IDs can collide | `scripts/tester-mql-remote.sh`, `scripts/optimize-mql-remote.sh` | Add unique run/worker identities and locks |
| Timeout may leave MT5 running remotely | Remote tester scripts | Reconcile remote ownership before another launch |
| Monte Carlo uses position-level realized P/L, not floating equity or dependent baskets | `scripts/monte-carlo.py` | Label limitations and add cycle/block analysis |
| EA has money-unit conversion and a custom tester criterion | `FlexUltimateGRH.mq5` | Verify denomination handling; preserve criterion and avoid double conversion |

## 2. Architecture and ownership

```text
Zed / MCP / CLI
       |
       v
Persistent research controller
       +-- Campaign definitions and parameter search
       +-- Durable queue, worker locks, retries and recovery
       +-- Validation and candidate ranking
       +-- SQLite index + immutable run artifacts
                         |
                         v
               Dedicated MT5 workers
                  +-- Lirunex
                  +-- Exness later
```

Extend the existing Rust/Python stack for the first version. Keep the controller independent of the language server and the lifetime of a Zed MCP connection. SQLite is a proposed new dependency for campaigns, candidates, jobs, transitions, and decisions; retain `.mql/runs/` for immutable artifacts. Final library selection belongs to implementation review.

The AI may propose experiments and explain evidence. Deterministic code validates inputs, enforces risk/resource limits, calculates metrics, and decides eligibility. Logs, EA comments, and reports are untrusted data, not instructions. Model unavailability must not corrupt job state or bypass validation.

A campaign identifies broker/server, account denomination and contract profile, symbol, strategy, timeframes, deposit, leverage, risk policy, source snapshot, search ranges, and data partitions. A test account profile is not automatically equivalent to another profile with the same currency label.

The controller can start on the Mac for development. Closing Zed must not stop it; Mac sleep or shutdown still interrupts scheduling. Fully unattended scheduling requires an always-on supported host and an explicit service setup decision.

## 3. Phase 1: trustworthy single-worker execution

- [ ] Replace MCP placeholders with real doctor, lint, backtest, run retrieval, and comparison operations.
- [ ] Use typed requests, registered workspaces/workers, approved paths, and non-executable configuration. Reject arbitrary hosts, commands, and path escapes at local and remote boundaries.
- [ ] Persist accepted jobs before launch; fix failure transitions; bound output, runtime, queue length, artifact size, and pass counts server-side.
- [ ] Track owned local/remote execution, lock the worker, implement cancellation and restart reconciliation, and retain partial evidence. Mark uncertain remote state explicitly; never retry blindly.
- [ ] Generate collision-resistant IDs and reject stale/missing reports. Refuse to close unrelated terminals and keep paid/cloud agents disabled.

**Acceptance:** a real test produces verifiable artifacts; a failed job stays failed; controller restart does not duplicate active execution; cancellation cannot affect another terminal.

## 4. Phase 2: reproducible EA measurements

- [ ] Capture source/include hashes, compiled hash, full inputs, broker/server, symbol specifications, terminal build, dates, model, costs, deposit, leverage, and available history provenance. State when history is not archived and exact replay is unavailable.
- [ ] Verify EA cent/base-money conversion and broker contract sizes/minimum lots. Reject unaffordable sizing rather than rounding volume up or converting money inputs twice.
- [ ] Integrate structured tester statistics and cycle telemetry while preserving strategy behavior and the existing `OnTester` criterion. Review instrumentation changes before freezing the campaign source.
- [ ] Record native equity drawdown, sampled equity separately, profit, exposure, margin, cycle counts, recovery duration, safety halts, and unfinished exposure. Missing critical metrics disqualify a result from ranking.
- [ ] Compare a baseline with MT5 output and repeat it under identical conditions. Preserve initialization failures and zero-trade outcomes as distinct from infrastructure errors.

**Acceptance:** baseline metrics match independently inspected tester output within documented tolerances; closed profit cannot conceal unreported floating loss; the snapshot tested is identifiable.

## 5. Phase 3: continuous parameter research

- [ ] Define campaign schemas and strategy-specific parameter ranges, enum rules, conditional dependencies, and locked risk bounds. Do not tune inactive strategy parameters.
- [ ] Begin with seeded random exploration and bounded refinement around promising regions. Retain exploration budget rather than searching only around the latest winner.
- [ ] Store every candidate, attempt, seed, proposal reason, outcome, and rejection reason. Deduplicate by source, normalized inputs, broker conditions, data identity, and date/model configuration.
- [ ] Implement bounded batches, resource budgets, graceful pause after active jobs, immediate cancellation requests, resumable search state, and automatic pause after repeated infrastructure failures.
- [ ] Add native MT5 optimization as an optional backend only after per-pass collection is proven; rerun shortlisted passes individually for detailed equity/cycle artifacts.

**Acceptance:** a campaign completes successive batches and resumes its history after restart without rerunning completed equivalent experiments or relaxing risk limits. Exhausted search space becomes an explicit idle/completed state, not an infinite duplicate loop.

## 6. Phase 4: robustness validation and selection

- [ ] Freeze chronological training/validation windows before search, with indicator warmup and explicit treatment of baskets crossing boundaries. Use rolling validation and a separately protected final holdout.
- [ ] Prevent final-holdout results from feeding repeated parameter selection. Record all attempted configurations and disclose selection bias; continued search must not silently reuse a consumed holdout as untouched evidence.
- [ ] Check nearby parameter settings, market regimes, realistic cost/execution stress, and real ticks where available for final candidates. Record which stress assumptions are modeled versus actually rerun in MT5.
- [ ] Add cycle/block resampling and supported-structure checks. Label realized-P/L simulations separately from equity/margin risk; do not present scenario frequencies as future ruin probabilities.
- [ ] Maintain a shortlist based on profit, drawdown, consistency, recovery duration, and completed-cycle sample adequacy. Report uncertainty and allow no qualifying candidate rather than loosening thresholds.

**Acceptance:** training winners can fail validation; insufficient samples or missing risk metrics cannot pass; every exported candidate links to independent evidence, costs, limitations, and exact inputs. Existing custom tester scores are supporting evidence, not the sole ranking rule.

## 7. Phase 5: parallel brokers, markets, and operation

- [ ] Register dedicated Lirunex and Exness workers; verify account profile, symbol mapping, installation/data paths, data availability, and supported resource capacity.
- [ ] Isolate tester configuration, exports, scheduled tasks, deployed artifacts, and EA state. Explicitly address shared `Common/Files`; different terminal data folders alone are insufficient evidence of isolation.
- [ ] Enforce one active job per worker initially and cap aggregate resource use, including native tester agents. Use fair scheduling across campaigns and quarantine unhealthy workers.
- [ ] Expose status, pending/running jobs, validation progress, rejection reasons, shortlist, pause/resume, and export through CLI/MCP. Defer a richer dashboard until execution is reliable.
- [ ] Export selected `.set` files and an evidence bundle for manual review/demo forward testing. Never attach an EA to a live chart or modify the future live installation.

**Acceptance:** simultaneous tests of the same EA cannot overwrite or consume each other's artifacts; interruption is recoverable; neither worker can launch, stop, or change a live terminal.

## 8. Verification strategy

| Layer | Required tests |
|---|---|
| Unit/fixtures | Input dependencies, currency conversion, deduplication, status transitions, ranking, malformed/partial reports, seeded search |
| Mock runner | Process failure, lost SSH, timeout, delayed completion, cancellation, controller restart, stale artifacts |
| Ownership/isolation | Same-EA concurrent jobs, path escape rejection, worker locks, shared-export isolation, live-path rejection |
| Windows integration | Compile current snapshot, verify a real baseline, compare native metrics, restart/reconcile, cancel only owned execution |
| Research flow | Multiple batches, pause/resume, separate validation partitions, candidate rejection, evidence-linked export |

Use existing Rust and Python test infrastructure where applicable. Add MCP/controller-specific coverage; no test files were found in `mql-mcp` during this audit. Mock/local tests do not replace Windows/MT5 verification. Record commands, versions, outcomes, and limitations when implementation is tested.

## 9. First milestone and remaining decisions

**First working milestone:** one Lirunex cent-account campaign, one symbol, one entry mode, 10,000-cent deposit, approved fixed risk/exposure rules, verified metrics, durable history, and reliable pause/resume. Add Exness and parallel markets only after this passes.

| Decision | Proposed handling |
|---|---|
| First symbol/entry mode | Choose after checking Lirunex history coverage and baseline validity |
| USD-account drawdown ceiling | Require approval; do not inherit the cent ceiling implicitly |
| Data ranges and sample minimums | Select from verified history and cycle frequency; freeze before search |
| Always-on host | Mac for development; choose an always-on host before unattended scheduling |
| Exposure/recovery limits | Approve lot, margin, cycle-loss, and recovery-duration bounds before optimization |

Implementation order: Phase 1 execution integrity, Phase 2 measurements, Phase 3 research loop, Phase 4 validation, then Phase 5 parallel expansion. Validation rules and data partitions must be specified before accepting any Phase 3 result as a qualified preset.
