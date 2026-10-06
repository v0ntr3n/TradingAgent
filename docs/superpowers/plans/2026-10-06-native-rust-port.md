# Native Rust Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a native Rust implementation of the TradingAgent core beside the existing Python runtime, preserving v0.6 behavior, the crypto integrations already ported from Tomortec, and the missing Tomortec core features identified in the audit.

**Architecture:** Add a Rust workspace under `rust/` with separate core, data, LLM, reporting, and CLI crates. The core uses typed serde state and a Tokio workflow rather than reproducing LangGraph internals; data/LLM/reporting are accessed through project-owned traits so the workflow remains independent of vendor SDKs. Python remains unchanged as the reference runtime until deterministic parity tests pass.

**Tech Stack:** Rust 2024 edition; Tokio; serde/serde_json; chrono; rust_decimal; reqwest; async-trait; thiserror; schemars; clap; tracing; sha2; tempfile; wiremock/httpmock for HTTP tests; cargo-nextest optional locally but `cargo test` is the CI baseline.

**Spec:** `docs/superpowers/specs/2026-10-06-rust-port-design.md`

## Global Constraints

- Implement under `rust/`; do not remove or route production Python entry points to Rust during this plan.
- Email delivery and PDF generation are out of scope.
- Historical runs must not consume live/current-only crypto data; enforcement belongs below the agent layer.
- Persisted run state and checkpoints must carry an explicit schema version and reject unsupported versions.
- Secrets and credential-bearing URLs must not be written to reports, logs, checkpoints, or run-settings metadata.
- Preserve the v0.6 five-tier decision scale (`Buy`, `Overweight`, `Hold`, `Underweight`, `Sell`) and explicit review/failure semantics where a valid rating cannot be produced.
- Preserve quick/deep model tiers and allow each tier to select its own provider, model, and endpoint.
- OpenAI-compatible endpoints are required; Qwen/DashScope compatible mode must work through that path.
- Native provider families in current v0.6 (Anthropic, Google, Azure OpenAI, AWS Bedrock) are parity requirements, implemented behind the same Rust LLM trait.
- Current general data families (Yahoo, SEC EDGAR, Alpha Vantage, FRED, Polymarket, Reddit/Stocktwits-equivalent social data) remain supported through Rust data traits/router semantics.
- Crypto integrations include Binance USD-M futures enrichment, TAAPI, Alternative.me Fear & Greed, CoinStats, CoinDesk, BlockBeats, and Reddit-equivalent crypto context.
- Trader output includes action, reasoning, entry price, support, resistance, take-profit, stop-loss, and position sizing; absent evidence produces `None`, never invented numeric defaults.
- Investment preferences, external reports, and portfolio context are distinct first-class inputs.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace` must pass before the branch is merge-ready.

## Review Focus

1. **Historical/live boundary at local date rollover:** a run whose `trade_date` is before the runtime's effective current date must receive `WithheldHistorical` from live-only providers, including when UTC and local dates differ. Task 3 owns the regression tests.
2. **Hostile external research text:** pasted reports containing prompt-like instructions must be serialized as untrusted evidence and never become system/developer instructions. Task 2 and Task 7 own the tests.
3. **Malformed structured financial values:** percentages/ranges/NaN/infinity in absolute price fields must fail validation or become `None`; they must never silently coerce to plausible prices. Task 2 owns the tests.
4. **Credential leakage through endpoints/logging:** API keys in headers/query strings or credentials embedded in base URLs must be redacted from diagnostics and persistence. Task 6 and Task 9 own the tests.
5. **Resume under changed settings:** changing analysts, debate depth, provider/model/endpoints, output language, portfolio, or preferences must invalidate an old checkpoint rather than resume stale state. Task 9 owns the tests.

---

## File Structure

The implementation creates the following primary units.

```text
rust/
├── Cargo.toml
├── crates/
│   ├── trading-agent-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── symbol.rs
│   │       ├── config.rs
│   │       ├── state.rs
│   │       ├── preferences.rs
│   │       ├── external.rs
│   │       ├── portfolio.rs
│   │       ├── decision.rs
│   │       ├── events.rs
│   │       ├── checkpoint.rs
│   │       ├── memory.rs
│   │       ├── agents/
│   │       │   ├── mod.rs
│   │       │   ├── analysts.rs
│   │       │   ├── researchers.rs
│   │       │   ├── trader.rs
│   │       │   ├── risk.rs
│   │       │   └── managers.rs
│   │       └── workflow/
│   │           ├── mod.rs
│   │           ├── runner.rs
│   │           └── debate.rs
│   ├── trading-agent-data/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs
│   │       ├── traits.rs
│   │       ├── policy.rs
│   │       ├── router.rs
│   │       └── vendors/
│   │           ├── mod.rs
│   │           ├── binance.rs
│   │           ├── taapi.rs
│   │           ├── alternative_me.rs
│   │           ├── coinstats.rs
│   │           ├── coindesk.rs
│   │           ├── blockbeats.rs
│   │           ├── yahoo.rs
│   │           ├── sec_edgar.rs
│   │           ├── alpha_vantage.rs
│   │           ├── fred.rs
│   │           ├── polymarket.rs
│   │           └── social.rs
│   ├── trading-agent-llm/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs
│   │       ├── client.rs
│   │       ├── registry.rs
│   │       ├── structured.rs
│   │       ├── research.rs
│   │       └── providers/
│   │           ├── mod.rs
│   │           ├── openai_compatible.rs
│   │           ├── anthropic.rs
│   │           ├── google.rs
│   │           ├── azure.rs
│   │           └── bedrock.rs
│   ├── trading-agent-reporting/
│   │   ├── Cargo.toml
│   │   └── src/{lib.rs,markdown.rs,html.rs,persistence.rs}
│   └── trading-agent-cli/
│       ├── Cargo.toml
│       └── src/{main.rs,args.rs,config.rs,run.rs}
└── tests/
    ├── fixtures/
    └── parity/
```

---

### Task 1: Rust Workspace, Core Configuration, Symbols, and Base State

**Files:**
- Create: `rust/Cargo.toml`
- Create: `rust/crates/trading-agent-core/Cargo.toml`
- Create: `rust/crates/trading-agent-core/src/{lib.rs,error.rs,symbol.rs,config.rs,state.rs}`
- Test: inline unit tests in the new modules

**Interfaces:**
- Produces: `Symbol::parse(&str) -> Result<Symbol, CoreError>`
- Produces: `Symbol::asset_type(&self) -> AssetType`
- Produces: `RunConfig`, `ModelTierConfig`, `AgentState`, `AssetType`, `SCHEMA_VERSION`
- `RunConfig` includes analysts, debate/risk/tool round limits, output language, quick/deep tier settings, checkpoint toggle, vendor routing configuration, and retry/token/temperature settings.

- [ ] **Step 1: Write failing symbol and config tests**

Add tests asserting `BTC`, `BTCUSD`, `BTC-USDT`, and `BTC-USD` normalize to a canonical crypto symbol; stock suffixes remain stocks; invalid/empty symbols fail; and `RunConfig::fingerprint_material()` excludes secret values while changing when behavioral settings change.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd rust && cargo test -p trading-agent-core symbol config`
Expected: FAIL because the workspace/types do not exist yet.

- [ ] **Step 3: Implement workspace and minimal core types**

Implement exact public interfaces above using serde-compatible types. `AgentState` initially contains schema/version, identity/date, input contexts, optional report fields, debate/decision fields, and past-memory context as defined by the spec.

- [ ] **Step 4: Run focused tests**

Run: `cd rust && cargo test -p trading-agent-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/Cargo.toml rust/crates/trading-agent-core
git commit -m "feat(rust): add core workspace and state types"
```

---

### Task 2: Preferences, External Reports, Portfolio, and Structured Decisions

**Files:**
- Create: `rust/crates/trading-agent-core/src/preferences.rs`
- Create: `rust/crates/trading-agent-core/src/external.rs`
- Create: `rust/crates/trading-agent-core/src/portfolio.rs`
- Create: `rust/crates/trading-agent-core/src/decision.rs`
- Modify: `rust/crates/trading-agent-core/src/lib.rs`
- Test: inline module tests

**Interfaces:**
- Produces: `InvestmentPreferences::from_json_str(&str) -> Result<Self, CoreError>` and `render_for_prompt(&self) -> String`
- Produces: `ExternalReport { title: Option<String>, source: Option<String>, content: String }` and `render_untrusted(&self) -> String`
- Produces: `PortfolioContext::fingerprint() -> String` and `render(&self, symbol: &Symbol) -> String`
- Produces: `TraderProposal::validate(self) -> Result<Self, CoreError>` with decimal-safe price fields.

- [ ] **Step 1: Write failing tests for all input models**

Assert Tomortec-style futures/aggressive/horizon/leverage/max-loss preferences round-trip; portfolio distinguishes no context vs flat vs positions; hostile external text remains inside a clearly delimited `UNTRUSTED EXTERNAL RESEARCH` block; `TraderProposal` rejects non-finite/percentage/range strings before they can become absolute price values; valid decimal levels preserve precision.

- [ ] **Step 2: Run tests to verify failure**

Run: `cd rust && cargo test -p trading-agent-core preferences external portfolio decision`
Expected: FAIL with missing modules/types.

- [ ] **Step 3: Implement minimal typed models and rendering**

Use `rust_decimal::Decimal` for tradable prices. Keep `free_form` preferences but never treat external-report contents as instructions.

- [ ] **Step 4: Run focused tests**

Run: `cd rust && cargo test -p trading-agent-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-core
git commit -m "feat(rust): add run inputs and structured decisions"
```

---

### Task 3: Data Contracts, Point-in-Time Policy, and Vendor Router

**Files:**
- Create: `rust/crates/trading-agent-data/Cargo.toml`
- Create: `rust/crates/trading-agent-data/src/{lib.rs,types.rs,traits.rs,policy.rs,router.rs}`
- Test: inline module tests

**Interfaces:**
- Produces: `DataStatus<T> = Available(T) | Unavailable { source, reason } | WithheldHistorical { source, as_of }`
- Produces traits: `MarketDataSource`, `TechnicalIndicatorSource`, `NewsSource`, `SentimentSource`, `FundamentalsSource`, `MacroSource`
- Produces: `DataPolicy::classify(as_of: NaiveDate, availability: AvailabilityClass, now: DateTime<FixedOffset>) -> DataAccess`
- Produces: `VendorRouter` with exact configured chains and no silent unconfigured fallbacks.

- [ ] **Step 1: Write failing policy/router tests**

Cover current vs historical dates, timezone rollover, current-only vs archival sources, empty vendor chains, explicit fallback ordering, and prevention of silent vendor substitution.

- [ ] **Step 2: Run tests to prove red state**

Run: `cd rust && cargo test -p trading-agent-data policy router`
Expected: FAIL because crate/interfaces are missing.

- [ ] **Step 3: Implement contracts and policy**

Make policy deterministic by passing `now` explicitly in tests/runtime. Provider implementations return typed `DataStatus` rather than prose sentinel strings.

- [ ] **Step 4: Run focused tests**

Run: `cd rust && cargo test -p trading-agent-data`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-data
git commit -m "feat(rust): add point-in-time data contracts"
```

---

### Task 4: Crypto Vendor Integrations

**Files:**
- Create: `rust/crates/trading-agent-data/src/vendors/{mod.rs,binance.rs,taapi.rs,alternative_me.rs,coinstats.rs,coindesk.rs,blockbeats.rs}`
- Add fixture files under: `rust/tests/fixtures/crypto/`
- Test: vendor module tests using a local mock HTTP server

**Interfaces:**
- Produces provider structs implementing Task 3 traits.
- Binance exposes normalized USD-M symbol mapping and parses candles, best bid/ask, 24h stats, top/global long-short ratios, and taker ratio into typed snapshots.
- TAAPI exposes bulk indicator requests for configured timeframe/indicator sets.

- [ ] **Step 1: Add fixture-driven failing parser/request tests**

Assert `BTC-USD -> BTCUSDT`, Binance endpoint/path/query construction, TAAPI bearer auth and `/bulk` payload shape, Alternative.me parsing, CoinStats BTC dominance/news, CoinDesk news, and BlockBeats flash parsing. Assert current-only providers return historical withholding through the router without sending HTTP.

- [ ] **Step 2: Run focused tests and confirm failure**

Run: `cd rust && cargo test -p trading-agent-data vendors::`
Expected: FAIL because providers are absent.

- [ ] **Step 3: Implement crypto providers with `reqwest`**

Use injected base URLs/clients for testability and bounded timeouts. Do not log raw auth headers or API-key query parameters.

- [ ] **Step 4: Run crypto provider tests**

Run: `cd rust && cargo test -p trading-agent-data vendors::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-data rust/tests/fixtures/crypto
git commit -m "feat(rust): port crypto data providers"
```

---

### Task 5: Stock and General Data Providers

**Files:**
- Create: `rust/crates/trading-agent-data/src/vendors/{yahoo.rs,sec_edgar.rs,alpha_vantage.rs,fred.rs,polymarket.rs,social.rs}`
- Add fixtures under: `rust/tests/fixtures/general/`
- Modify: `rust/crates/trading-agent-data/src/vendors/mod.rs`

**Interfaces:**
- Yahoo: OHLCV/snapshot/news/fundamentals adapter through Task 3 traits.
- SEC EDGAR: statement/fundamentals path with knowledge-time metadata.
- Alpha Vantage: optional stock/indicator/news/fundamental path.
- FRED: macro series provider.
- Polymarket: prediction-market context provider.
- Social: Reddit/Stocktwits-equivalent typed social context adapter.

- [ ] **Step 1: Write failing fixture tests for normalized outputs and date rules**

Include stale OHLCV rejection, statement knowledge-time cutoff, source-specific unavailable errors, ordered router fallback, and social fallback behavior matching Python's fail-open semantics where safe.

- [ ] **Step 2: Run tests and verify red state**

Run: `cd rust && cargo test -p trading-agent-data general_providers`
Expected: FAIL until providers exist.

- [ ] **Step 3: Implement provider adapters**

Keep transport/parsing inside the data crate and return typed normalized models; do not expose vendor JSON to agents.

- [ ] **Step 4: Run the whole data crate**

Run: `cd rust && cargo test -p trading-agent-data`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-data rust/tests/fixtures/general
git commit -m "feat(rust): port stock and general data providers"
```

---

### Task 6: LLM Abstraction, Provider Registry, Structured Output, and Research Capability

**Files:**
- Create: `rust/crates/trading-agent-llm/Cargo.toml`
- Create: `rust/crates/trading-agent-llm/src/{lib.rs,types.rs,client.rs,registry.rs,structured.rs,research.rs}`
- Create: `rust/crates/trading-agent-llm/src/providers/{mod.rs,openai_compatible.rs,anthropic.rs,google.rs,azure.rs,bedrock.rs}`
- Test: module tests with mock HTTP clients/servers and provider request snapshots

**Interfaces:**
- Produces: `LlmClient::complete(&self, LlmRequest) -> Result<LlmResponse, LlmError>`
- Produces object-safe structured helper `complete_json(&self, request: LlmRequest, schema: &RootSchema) -> Result<serde_json::Value, LlmError>`; generic deserialization lives in `structured::complete_structured<T>()` to avoid object-safety issues.
- Produces: `ResearchClient::research(&self, ResearchRequest) -> Result<ResearchResponse, LlmError>`
- Produces: `ProviderRegistry::create(&ProviderConfig) -> Result<Arc<dyn LlmClient>, LlmError>`

- [ ] **Step 1: Write failing registry/request/redaction tests**

Cover OpenAI-compatible custom base URL + env key, Qwen/DashScope compatible configuration, quick/deep tiers on different providers, Anthropic/Google/Azure/Bedrock registry selection, structured schema round-trip, unsupported provider errors, retries/token limits/temperature wiring, and redaction of embedded URL credentials/API keys in errors.

- [ ] **Step 2: Run LLM tests to verify failure**

Run: `cd rust && cargo test -p trading-agent-llm`
Expected: FAIL because crate is absent.

- [ ] **Step 3: Implement provider-neutral interfaces and adapters**

Use project-owned request/response types. Provider-specific payload construction remains inside provider modules. Research capability advertises whether point-in-time semantics are supported and declines unsafe historical requests.

- [ ] **Step 4: Run LLM crate tests**

Run: `cd rust && cargo test -p trading-agent-llm`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-llm
git commit -m "feat(rust): add LLM provider abstraction"
```

---

### Task 7: Analyst, Researcher, Trader, Risk, and Manager Agents

**Files:**
- Create: `rust/crates/trading-agent-core/src/agents/{mod.rs,analysts.rs,researchers.rs,trader.rs,risk.rs,managers.rs}`
- Modify: `rust/crates/trading-agent-core/Cargo.toml`
- Test: agent module tests using deterministic fake LLM/data contexts

**Interfaces:**
- Produces async agent functions returning typed report/decision values rather than mutating global state.
- Analyst context consumes Task 2 inputs and Task 3 normalized evidence.
- Fundamentals supports both stocks and crypto; crypto fundamentals can consume BTC dominance plus optional safe research capability.

- [ ] **Step 1: Write failing prompt/context tests**

Assert language instruction propagation; investment preferences reach market/research/trader/risk/manager stages; external reports are included only inside untrusted evidence delimiters; portfolio reaches trader/risk/portfolio-manager; crypto fundamentals is available; unavailable/withheld evidence is stated rather than invented; trader asks for all seven structured output fields.

- [ ] **Step 2: Run agent tests to establish red state**

Run: `cd rust && cargo test -p trading-agent-core agents::`
Expected: FAIL because agent modules are absent.

- [ ] **Step 3: Implement minimal agents against `Arc<dyn LlmClient>`**

Keep prompts close to the current Python v0.6 intent while using typed Rust context. Do not add extra autonomous web/network access inside agent code.

- [ ] **Step 4: Run agent tests**

Run: `cd rust && cargo test -p trading-agent-core agents::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-core
git commit -m "feat(rust): port trading agents"
```

---

### Task 8: Native Tokio Workflow, Debate Routing, and Rating Semantics

**Files:**
- Create: `rust/crates/trading-agent-core/src/workflow/{mod.rs,runner.rs,debate.rs}`
- Modify: `rust/crates/trading-agent-core/src/state.rs`
- Modify: `rust/crates/trading-agent-core/src/lib.rs`
- Test: workflow integration tests in `rust/crates/trading-agent-core/tests/workflow.rs`

**Interfaces:**
- Produces: `WorkflowRunner::run(RunInput) -> Result<RunResult, CoreError>`
- Produces: `RunResult { state: AgentState, rating: FinalRating }`
- Analysts start concurrently and join before research debate; debate/risk rounds follow configured deterministic ordering.

- [ ] **Step 1: Write failing deterministic workflow tests**

Use barrier/instrumented fake agents to prove analyst concurrency; verify selected analysts only; verify bull/bear round counts; risk turn ordering; crypto can include Fundamentals; structured manager rating uses five-tier scale; malformed/no rating becomes `Review` rather than defaulting to Hold.

- [ ] **Step 2: Run workflow tests and verify failure**

Run: `cd rust && cargo test -p trading-agent-core --test workflow`
Expected: FAIL until runner exists.

- [ ] **Step 3: Implement Tokio orchestration and deterministic routers**

Workflow transitions must be enum/function driven, never inferred from generated prose except validated structured outputs.

- [ ] **Step 4: Run all core tests**

Run: `cd rust && cargo test -p trading-agent-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-core
git commit -m "feat(rust): add native async trading workflow"
```

---

### Task 9: Events, Checkpoint/Resume, Persistence, Markdown, and HTML Reporting

**Files:**
- Create: `rust/crates/trading-agent-core/src/{events.rs,checkpoint.rs}`
- Create: `rust/crates/trading-agent-reporting/Cargo.toml`
- Create: `rust/crates/trading-agent-reporting/src/{lib.rs,markdown.rs,html.rs,persistence.rs}`
- Modify: `rust/crates/trading-agent-core/src/workflow/runner.rs`
- Test: core/reporting integration tests

**Interfaces:**
- Produces: `RunEvent` enum and `EventSink` trait.
- Produces: `CheckpointStore::{load,save,clear}` and `CheckpointEnvelope { schema_version, signature, completed_stage, state }`.
- Produces: `RunSignature::from_inputs(config, portfolio, preferences, external_reports_metadata) -> String`.
- Produces: `ReportWriter::write_event` and `write_complete` with the spec's directory layout.

- [ ] **Step 1: Write failing checkpoint/event/report tests**

Assert stable-stage checkpoint writes; unsupported schema refusal; changed analysts/depth/provider/model/endpoint/language/portfolio/preferences invalidate resume; secrets do not appear in checkpoint/state/report/log; partial report files are updated as events arrive; complete Markdown and self-contained HTML are produced; successful run clears active checkpoint.

- [ ] **Step 2: Run tests and verify failure**

Run: `cd rust && cargo test -p trading-agent-core checkpoint && cargo test -p trading-agent-reporting`
Expected: FAIL because APIs are missing.

- [ ] **Step 3: Implement event stream, checkpoint store, and reporting**

Use atomic temp-file + rename writes for checkpoint/state/report artifacts where the platform permits. Redact URLs/headers through shared safe-display helpers.

- [ ] **Step 4: Run core/reporting tests**

Run: `cd rust && cargo test -p trading-agent-core && cargo test -p trading-agent-reporting`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-core rust/crates/trading-agent-reporting
git commit -m "feat(rust): add checkpointing and reports"
```

---

### Task 10: Decision Memory, Settlement, and Reflection

**Files:**
- Create: `rust/crates/trading-agent-core/src/memory.rs`
- Add: `rust/crates/trading-agent-core/tests/memory.rs`
- Modify: `rust/crates/trading-agent-core/src/workflow/runner.rs`

**Interfaces:**
- Produces: `DecisionMemory` trait with `store_decision`, `pending`, `settle`, and `context_as_of`.
- Produces initial file-backed implementation `JsonDecisionMemory` (SQLite may be added later without changing the trait).
- Produces settlement types carrying resolution date, outcome, alpha/benchmark context when available, and reflection text.

- [ ] **Step 1: Write failing point-in-time memory tests**

Assert current run decision remains pending; later runs can settle it; historical `context_as_of` excludes lessons resolved after the requested date; cross-ticker lessons do not overwrite same-ticker context; failed settlement stays pending and does not abort otherwise valid analysis.

- [ ] **Step 2: Run tests to verify red state**

Run: `cd rust && cargo test -p trading-agent-core --test memory`
Expected: FAIL until memory exists.

- [ ] **Step 3: Implement JSON-backed memory and workflow hooks**

Keep storage behind the trait and use lock/atomic-write protection sufficient for a single-machine scheduler.

- [ ] **Step 4: Run memory/core tests**

Run: `cd rust && cargo test -p trading-agent-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-core
git commit -m "feat(rust): add decision memory and reflection"
```

---

### Task 11: Rust CLI and Configuration Precedence

**Files:**
- Create: `rust/crates/trading-agent-cli/Cargo.toml`
- Create: `rust/crates/trading-agent-cli/src/{main.rs,args.rs,config.rs,run.rs}`
- Add: `rust/crates/trading-agent-cli/tests/cli.rs`
- Modify: `rust/Cargo.toml`

**Interfaces:**
- Binary: `trading-agent`
- Required non-interactive inputs: ticker/date/analysts plus provider/model settings from CLI/env/config/default precedence.
- Flags include preferences file, repeated external-report files, portfolio file, output language, debate/risk depth, checkpoint, report save/show, quick/deep provider/model/base URLs.

- [ ] **Step 1: Write failing `assert_cmd` CLI tests**

Assert headless parse, asset auto-detection, crypto Fundamentals allowed, invalid ticker/date failure, CLI > env > config > default precedence, secret-safe diagnostics, preferences/external/portfolio file parsing, and deterministic no-prompt operation when all required inputs are supplied.

- [ ] **Step 2: Run CLI tests and verify failure**

Run: `cd rust && cargo test -p trading-agent-cli`
Expected: FAIL because CLI crate is absent.

- [ ] **Step 3: Implement clap CLI/config loader and run wiring**

Interactive prompting may be minimal in the first port; non-interactive parity is required and must not depend on TTY availability.

- [ ] **Step 4: Run CLI tests**

Run: `cd rust && cargo test -p trading-agent-cli`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-cli rust/Cargo.toml
git commit -m "feat(rust): add native CLI"
```

---

### Task 12: Backtest Runtime

**Files:**
- Create: `rust/crates/trading-agent-core/src/backtest.rs`
- Modify: `rust/crates/trading-agent-core/src/lib.rs`
- Add: `rust/crates/trading-agent-core/tests/backtest.rs`

**Interfaces:**
- Produces: `BacktestRunner::run(BacktestRequest) -> Result<BacktestResult, CoreError>`
- `BacktestRequest` supplies symbol, date sequence/window, initial capital/position policy, and the same `RunConfig` used by live analysis.
- Uses point-in-time data policy and the workflow runner; does not call live-only crypto enrichments for historical dates.

- [ ] **Step 1: Write failing deterministic backtest tests**

Use fake historical prices/workflow outputs to verify date ordering, no look-ahead, cash/position evolution, skipped unavailable days, benchmark/alpha calculation where configured, and deterministic summary metrics.

- [ ] **Step 2: Run tests and verify red state**

Run: `cd rust && cargo test -p trading-agent-core --test backtest`
Expected: FAIL until runner exists.

- [ ] **Step 3: Implement backtest runner**

Reuse workflow/data interfaces; no alternate hidden data path is permitted.

- [ ] **Step 4: Run core tests**

Run: `cd rust && cargo test -p trading-agent-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/trading-agent-core
git commit -m "feat(rust): port backtest runtime"
```

---

### Task 13: Python/Rust Parity Fixtures and End-to-End Mock Runs

**Files:**
- Create: `rust/tests/fixtures/parity/{stock_run.json,crypto_run.json,historical_crypto_run.json}`
- Create: `rust/tests/parity/{common.rs,stock.rs,crypto.rs}`
- Create: `tests/rust_parity_fixture.py` only if a small Python fixture exporter is necessary; do not change Python production runtime.
- Modify: Rust dev-dependencies as needed.

**Interfaces:**
- Parity fixtures define normalized inputs, provider evidence, deterministic fake-LLM outputs, expected stage order, expected withholding decisions, and expected structured state fields.
- The parity contract compares semantics/state, not raw prose formatting.

- [ ] **Step 1: Write failing parity tests from frozen fixtures**

Stock fixture covers standard analysts, five-tier rating, portfolio context, reporting fields. Crypto fixture covers all crypto enrichments, restored Fundamentals, preferences/external reports, and full trader levels. Historical fixture proves all current-only crypto evidence is withheld.

- [ ] **Step 2: Run parity tests and verify red state**

Run: `cd rust && cargo test --test stock --test crypto`
Expected: FAIL until fixture harness/end-to-end composition is complete.

- [ ] **Step 3: Implement parity harness and any missing composition wiring**

If Python export is required, make it test-only and deterministic with mocked LLM/data inputs.

- [ ] **Step 4: Run entire Rust workspace**

Run: `cd rust && cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/tests rust/Cargo.toml tests/rust_parity_fixture.py
git commit -m "test(rust): add Python parity fixtures"
```

(If the Python helper was not needed, omit it from `git add`.)

---

### Task 14: Rust CI, Documentation, and Merge-Readiness Verification

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Create: `rust/README.md`
- Modify: `.gitignore` if `rust/target/` is not already covered.

**Interfaces:**
- CI adds a Rust job using a pinned stable toolchain and caches Cargo artifacts.
- README documents that Rust is additive/experimental until parity gate is met and shows build/test/headless CLI commands.

- [ ] **Step 1: Add CI/documentation expectations as tests/checks where possible**

Confirm CI invokes format, clippy with warnings denied, and full workspace tests. Document all required environment variables without sample secrets.

- [ ] **Step 2: Run formatting and static checks**

Run:

```bash
cd rust
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: both exit 0.

- [ ] **Step 3: Run the complete Rust test suite**

Run: `cd rust && cargo test --workspace`
Expected: 0 failed tests.

- [ ] **Step 4: Run the existing Python CI test/lint commands from `.github/workflows/ci.yml`**

Expected: existing Python checks remain green; the Rust port has not regressed `main` behavior.

- [ ] **Step 5: Inspect branch diff against `main`**

Verify Python production files are unchanged except documentation/CI and an optional test-only parity helper; verify no credentials, generated `target/`, or local result artifacts are committed.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/ci.yml README.md rust/README.md .gitignore
git commit -m "ci(rust): verify native port"
```

---

## Final Verification Gate

Before opening or merging a PR, run all of the following from a clean checkout of `rust-port`:

```bash
cd rust
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Then run the Python commands exactly as defined by the repository's current `.github/workflows/ci.yml`.

The branch is not described as fully ported or parity-complete unless all commands exit successfully and the Task 13 stock, live-crypto, and historical-crypto parity fixtures pass.
