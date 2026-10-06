# Native Rust Port Design

Date: 2026-10-06
Branch: `rust-port`
Base: `main@cc2629ea1b46b83a69a0326a3534cccc3821f958`

## 1. Purpose

Port the TradingAgent runtime to a native Rust implementation while keeping the existing Python implementation working and unchanged on `main` until behavioral parity is demonstrated.

The Rust port is not a line-by-line translation of LangGraph/LangChain. It is a typed async reimplementation of the same trading workflow, preserving the modern TradingAgents v0.6 behavior, its current stock/data/provider capabilities, and the crypto functionality ported from Tomortec/CryptoTradingAgents. It also closes the remaining parity gaps identified during the audit: user investment preferences, user-supplied external reports, a crypto fundamentals path, and explicit support/resistance/take-profit trade levels.

Email delivery is intentionally out of scope.

## 2. Success criteria

The Rust port is considered ready to replace Python only when all of the following are true:

1. A stock run and a crypto run can execute end to end through the same logical agent stages as the Python system.
2. Existing v0.6 stock/data functionality has Rust equivalents, including Yahoo/yfinance-style market data, Alpha Vantage, SEC EDGAR fundamentals, FRED macro data, Polymarket, Reddit, Stocktwits, vendor routing, and explicit fallback-chain behavior.
3. Crypto data integrations cover Binance futures enrichment, TAAPI, Alternative.me Fear & Greed, CoinStats, CoinDesk, BlockBeats, Reddit/social context, and the existing historical look-ahead protections.
4. The Rust runtime supports investment preferences, external reports, portfolio context, multilingual output instructions, and structured final decisions.
5. Crypto runs can include a Fundamentals analyst rather than filtering that analyst out at CLI selection time.
6. Trader output contains action, reasoning, entry price, support, resistance, take-profit, stop-loss, and position sizing.
7. Checkpoint/resume, run-state persistence, report streaming, Markdown/HTML report generation, decision-memory settlement/reflection hooks, rating integrity, and backtesting have Rust equivalents.
8. LLM-provider parity covers the currently supported provider families: OpenAI, Anthropic, Google/Gemini, Azure OpenAI, AWS Bedrock, and the OpenAI-compatible provider registry used for compatible services such as Qwen/DashScope compatible mode, OpenRouter, Ollama, DeepSeek-style compatible endpoints, Gitee-compatible endpoints, and other configured compatible servers.
9. Separate quick/deep model tiers can use different providers and endpoints, as in the Python v0.6 runtime.
10. Historical runs cannot consume current-only crypto feeds or other data that violates point-in-time rules.
11. `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features` pass in CI.
12. Deterministic parity fixtures prove equivalent state transitions, routing, data-policy behavior, ratings, and persistence semantics between Python and Rust without requiring raw LLM prose to match byte-for-byte.

## 3. Migration strategy

Use a side-by-side Rust implementation under `rust/` on the `rust-port` branch. Python remains the reference implementation during the migration.

No Python production path is removed as part of the initial Rust port. The first Rust milestone is additive. Replacement or removal of Python is a separate decision after parity tests pass.

The repository will therefore contain two runtimes temporarily:

```text
TradingAgent/
├── tradingagents/              # existing Python runtime
├── cli/                        # existing Python CLI
├── tests/                      # existing Python tests
└── rust/
    ├── Cargo.toml
    ├── crates/
    └── tests/
```

## 4. Rust workspace layout

```text
rust/
├── Cargo.toml
├── crates/
│   ├── trading-agent-core/
│   │   └── src/
│   │       ├── state/
│   │       ├── workflow/
│   │       ├── agents/
│   │       ├── preferences/
│   │       ├── portfolio/
│   │       ├── memory/
│   │       ├── backtest/
│   │       ├── rating/
│   │       └── decision/
│   ├── trading-agent-data/
│   │   └── src/
│   │       ├── router/
│   │       ├── yahoo/
│   │       ├── alpha_vantage/
│   │       ├── sec_edgar/
│   │       ├── fred/
│   │       ├── polymarket/
│   │       ├── reddit/
│   │       ├── stocktwits/
│   │       ├── binance/
│   │       ├── taapi/
│   │       ├── coindesk/
│   │       ├── coinstats/
│   │       ├── blockbeats/
│   │       └── fear_greed/
│   ├── trading-agent-llm/
│   │   └── src/
│   │       ├── provider/
│   │       ├── openai/
│   │       ├── anthropic/
│   │       ├── google/
│   │       ├── azure/
│   │       ├── bedrock/
│   │       ├── openai_compatible/
│   │       ├── structured/
│   │       └── prompts/
│   ├── trading-agent-reporting/
│   │   └── src/
│   └── trading-agent-cli/
│       └── src/
└── tests/
    ├── fixtures/
    └── parity/
```

Each crate has one responsibility and communicates through typed public interfaces. The core crate does not perform HTTP directly. Data vendors do not know about agent prompts. Reporting consumes state/events but does not own workflow control.

## 5. Core state model

The Rust runtime uses explicit serde-serializable types rather than dictionary-style state.

A representative top-level state is:

```rust
pub struct AgentState {
    pub schema_version: u32,
    pub symbol: Symbol,
    pub asset_type: AssetType,
    pub trade_date: NaiveDate,
    pub instrument_context: InstrumentContext,

    pub investment_preferences: Option<InvestmentPreferences>,
    pub external_reports: Vec<ExternalReport>,
    pub portfolio: Option<PortfolioContext>,

    pub market_report: Option<MarketReport>,
    pub sentiment_report: Option<SentimentReport>,
    pub news_report: Option<NewsReport>,
    pub fundamentals_report: Option<FundamentalsReport>,

    pub bull_case: Option<ResearchCase>,
    pub bear_case: Option<ResearchCase>,
    pub research_plan: Option<ResearchPlan>,
    pub trader_proposal: Option<TraderProposal>,
    pub risk_debate: Option<RiskDebate>,
    pub final_decision: Option<PortfolioDecision>,

    pub past_context: Option<PastDecisionContext>,
}
```

Checkpoint and persisted run-state files include `schema_version`. Loading an unsupported schema version returns an explicit compatibility error; the runtime must not silently reinterpret stale state.

## 6. Investment preferences

The Rust port restores Tomortec's personalization capability as a typed input rather than a free-form string only.

The model supports fields such as:

- instrument/venue preference, such as spot or futures;
- whether long and/or short positions are permitted;
- trading style, such as conservative, balanced, or aggressive;
- intended holding horizon;
- leverage constraints;
- maximum acceptable loss or risk budget;
- free-form user instructions for requirements that do not map cleanly to a typed field.

Prompts receive a rendered form of these preferences. Decision code also receives the typed form so future risk validation can be deterministic rather than prompt-only.

## 7. External reports

Users may provide zero or more external research reports. Each report has an optional title/source plus text content.

External reports become first-class run inputs and are made available to the research and decision stages. They are advisory evidence, not a higher-priority instruction channel. LLM prompts must explicitly treat their contents as untrusted research text, not executable instructions, to reduce prompt-injection risk from pasted third-party material.

## 8. Workflow and orchestration

The Rust port uses a native Tokio-based workflow instead of recreating LangGraph internals.

Logical execution order:

```text
Market ─────────┐
Sentiment ──────┤
News ───────────┼──> Bull + Bear
Fundamentals ───┘        │
                         ▼
                 Research Manager
                         │
                       Trader
                         │
            ┌────────────┼────────────┐
         Aggressive    Neutral    Conservative
            └────────────┼────────────┘
                         ▼
                 Portfolio Manager
                         │
                         ▼
                  Final Decision
```

Independent analysts run concurrently using Tokio tasks. Their output is joined before bull/bear research begins. Bull and bear may also run concurrently when they are not waiting on each other's debate turns; configured debate rounds then alternate deterministically.

Risk analysts follow the configured risk-debate depth while preserving deterministic turn ordering.

Workflow transitions are encoded in Rust functions/enums, not parsed from LLM prose.

The current post-decision/rating path is preserved as typed logic so invalid or unparseable final decisions remain distinguishable from valid Buy/Overweight/Hold/Underweight/Sell ratings.

## 9. Agent boundaries

Each agent receives a typed context and an LLM interface. Agents do not own networking to market/news vendors.

Initial agent set:

- Market Analyst
- Sentiment Analyst
- News Analyst
- Fundamentals Analyst
- Bull Researcher
- Bear Researcher
- Research Manager
- Trader
- Aggressive Risk Analyst
- Neutral Risk Analyst
- Conservative Risk Analyst
- Portfolio Manager

The Fundamentals analyst is valid for both stocks and crypto. Stock fundamentals use the configured stock fundamentals providers. Crypto fundamentals combine crypto-market structure and macro/asset context such as BTC dominance plus search/research-capable LLM context when configured.

## 10. Data-provider interfaces and routing

Vendor integrations live behind small async traits. Example:

```rust
#[async_trait]
pub trait MarketDataSource: Send + Sync {
    async fn market_snapshot(
        &self,
        symbol: &Symbol,
        as_of: NaiveDate,
    ) -> Result<DataStatus<MarketSnapshot>, DataError>;
}
```

Other traits cover technical indicators, news, sentiment, fundamentals, macro data, prediction markets, and instrument identity.

The Rust data router preserves the Python v0.6 configuration model:

- category-level vendor configuration;
- tool-level overrides that take precedence over category defaults;
- explicit comma-separated/ordered fallback chains;
- no silent fallback to vendors the user did not configure;
- a `default` mode that intentionally opts into all supported vendors for a category.

Initial stock/general providers include:

- Yahoo/yfinance-compatible market, OHLCV, snapshot, news, and fundamentals data;
- Alpha Vantage market, technical, news, and fundamentals data;
- SEC EDGAR filed fundamentals/statements;
- FRED macroeconomic series;
- Polymarket prediction-market context;
- Reddit social/news context;
- Stocktwits social context.

Initial crypto providers include:

- Binance USD-M futures: candles, order book, 24h statistics, top/global long-short ratios, taker long-short ratio;
- TAAPI bulk technical indicators;
- Alternative.me Fear & Greed;
- CoinStats BTC dominance and news;
- CoinDesk news;
- BlockBeats flash news;
- Reddit/social context through the same abstract sentiment/news interfaces.

Yahoo/existing market sources remain the baseline market source where appropriate. Crypto enrichment augments rather than silently replaces baseline data.

## 11. Historical data safety

Point-in-time correctness is enforced below the agent layer.

Every provider declares whether a result is archival for the requested date or live/current-only. Current-only feeds return a withheld state for historical runs.

```rust
pub enum DataStatus<T> {
    Available(T),
    Unavailable { source: DataSource, reason: String },
    WithheldHistorical { source: DataSource, as_of: NaiveDate },
}
```

Agents may explain that information was unavailable or withheld, but they must never reconstruct a withheld live value through another prompt. The policy is implemented in provider/router code rather than relying only on prompt wording.

Existing knowledge-time protections for statements, news, undated tools, stale OHLCV, and social data are represented as deterministic provider/router policies and covered by parity fixtures.

## 12. LLM abstraction and provider parity

Agent code depends on a project-owned trait rather than directly on a third-party LLM crate:

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;

    async fn complete_structured<T>(
        &self,
        request: LlmRequest,
    ) -> Result<T, LlmError>
    where
        T: DeserializeOwned + JsonSchema + Send;
}
```

The initial Rust implementation includes provider adapters for:

- OpenAI;
- Anthropic;
- Google/Gemini;
- Azure OpenAI;
- AWS Bedrock;
- OpenAI-compatible providers through a registry and configurable base URL.

The OpenAI-compatible registry preserves compatible-provider behavior for Qwen/DashScope compatible mode, OpenRouter, Ollama, DeepSeek-style compatible endpoints, Gitee-compatible endpoints, and other configured compatible services.

Quick and deep model tiers retain independent provider/model/backend settings. Cross-provider options such as temperature, retry limits, and output-token caps are normalized in the project-owned configuration layer. Provider-specific reasoning/thinking options remain provider capabilities rather than leaking into agent code.

Structured outputs are schema-validated. A bounded fallback may retry or accept a carefully parsed textual response, but validation failures are surfaced explicitly rather than silently constructing default financial values.

## 13. Search-enabled research

Tomortec's separate search-LLM behavior is represented as a capability rather than hard-coded configuration keys.

A configured provider may expose a `ResearchClient` capable of current web/search-assisted research. The News and crypto Fundamentals analysts can consume this capability when present.

If no search-capable provider is configured, those agents continue with ordinary vendor evidence and label missing research instead of fabricating it.

Historical runs pass an explicit as-of date into research requests. Providers that cannot guarantee point-in-time search semantics must decline that request for historical analysis.

## 14. Structured trade proposal

The Rust trader restores the explicit decision levels present in Tomortec and combines them with v0.6's structured-output safety.

```rust
pub struct TraderProposal {
    pub action: TraderAction,
    pub reasoning: String,
    pub entry_price: Option<Decimal>,
    pub support: Option<Decimal>,
    pub resistance: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub position_sizing: Option<PositionSizing>,
}
```

Prices use decimal-safe numeric types rather than binary floating point where the value represents a tradable price.

No field is invented when evidence is insufficient. Missing levels remain `None` and render as `not provided`.

## 15. Portfolio context

The Rust port preserves the Python v0.6 portfolio semantics:

- signed quantities allow long and short positions;
- average price is optional;
- available cash and currency are optional;
- no portfolio context, a flat portfolio, and a non-flat portfolio remain distinct states.

Portfolio context is passed to Trader, risk analysts, and Portfolio Manager. It is not implicitly treated as investment preferences; the two inputs remain separate.

## 16. Reporting and event stream

The workflow emits typed events such as:

- run started;
- agent started/completed;
- tool/vendor request started/completed;
- report section updated;
- checkpoint written;
- run completed/failed.

The CLI subscribes to these events for progress output. Reporting subscribes independently and writes incremental artifacts.

Default run layout:

```text
results/
└── BTC-USD/
    └── 2026-10-06/
        ├── state.json
        ├── message_tool.log
        ├── reports/
        │   ├── market.md
        │   ├── sentiment.md
        │   ├── news.md
        │   ├── fundamentals.md
        │   ├── research.md
        │   ├── trader.md
        │   └── complete_report.md
        └── checkpoint.json
```

A self-contained HTML report is generated from the completed Markdown/state representation when enabled, matching the current Python reporting capability. PDF generation and email delivery are not required by this port.

## 17. Checkpoint/resume

Checkpoints are written at stable workflow boundaries, not during partially applied state mutations.

A checkpoint includes:

- schema version;
- symbol/date/asset type;
- a run-settings fingerprint;
- analyst selection and debate/risk depths;
- provider/model/vendor settings that affect generated state;
- portfolio fingerprint when portfolio context exists;
- completed workflow stage;
- serialized agent state.

Changing meaningful run settings invalidates the old checkpoint. Successful completion clears or archives the active checkpoint so a future run starts fresh unless the user explicitly requests reuse.

## 18. Memory, settlement, and reflection

The Rust port keeps the current v0.6 concept of past-decision memory rather than restoring Tomortec's older memory implementation verbatim.

Decision records include ticker, trade date, final decision/rating, and enough metadata to settle/reflect later. Historical runs only receive lessons whose resolution date is available as of the analysis date.

The runtime supports settlement of due same-ticker decisions and the all-pending settlement path used when a scheduler rotates tickers. Settlement failures remain pending and do not silently mark a decision as resolved.

The storage interface is abstract so the initial implementation can use SQLite/JSON-backed persistence without coupling agents to storage details.

## 19. Backtesting

The Rust port includes a backtest module that reuses the same workflow/data-policy interfaces rather than a separate analysis engine.

Backtesting preserves these current behaviors:

- historical trade dates use point-in-time data guards;
- configured holding period determines outcome measurement;
- benchmark/alpha calculations remain explicit and testable;
- final ratings/actions are recorded per run;
- memory/reflection only sees information available by the simulated date;
- deterministic fixtures can run without live APIs or live LLMs.

The Rust port does not add exchange execution or simulated brokerage fills beyond the analytical/backtest semantics already present in the Python project.

## 20. CLI

The first Rust CLI supports:

- ticker;
- analysis date;
- asset auto-detection;
- analyst selection;
- research/debate depth;
- quick/deep LLM provider and model selection;
- provider-specific or tier-specific base URLs where applicable;
- output language;
- investment-preferences file;
- external-report files;
- portfolio file;
- checkpoint enable/resume;
- report save/show options;
- non-interactive backtest invocation.

Crypto analyst selection includes Fundamentals. CLI defaults should match the Python runtime where that does not conflict with restored Tomortec behavior.

Non-interactive flags are required so parity and CI tests do not depend on terminal prompts. Exact reproduction of the Python Rich/questionary terminal presentation is not required; behavioral inputs and outputs are.

## 21. Configuration and secrets

Configuration is loaded from a combination of CLI arguments, environment variables, and optional config files with documented precedence.

The Rust configuration layer preserves the meaning of current `TRADINGAGENTS_*` settings where the Rust runtime implements the corresponding capability, including quick/deep providers, endpoints, output language, debate/risk/tool-round limits, checkpointing, temperature, retry limits, and token limits.

API secrets are read from environment variables or secret-safe configuration inputs. Secrets and credential-bearing backend URLs must not be persisted in reports, checkpoints, logs, or run-settings metadata.

Optional crypto keys include:

- `TAAPI_API_KEY`;
- `COINDESK_API_KEY`;
- `COINSTATS_API_KEY`.

Binance public market data, Alternative.me, and BlockBeats remain keyless where their public APIs permit it.

## 22. Error handling

Expected operational failures return typed errors; normal execution must not panic.

Error categories include:

- configuration error;
- invalid symbol/date/input;
- network timeout/transport error;
- provider authentication/rate-limit error;
- vendor parsing/schema error;
- unavailable or historically withheld data;
- LLM structured-output validation error;
- checkpoint compatibility error;
- report/persistence I/O error.

Vendor failures should degrade the relevant analyst evidence when safe to do so. Errors that make the final decision untrustworthy must fail the run explicitly rather than silently substituting data.

Retry policy is bounded, uses backoff for retryable transport/rate-limit failures, and does not retry deterministic validation/input errors.

## 23. Testing strategy

### Unit tests

Cover:

- stock/crypto symbol normalization and safe path components;
- date classification and historical withholding;
- investment-preference parsing/rendering;
- portfolio parsing/fingerprints;
- structured trade/rating validation;
- vendor-router precedence and fallback ordering;
- Yahoo, Alpha Vantage, SEC EDGAR, FRED, Polymarket, Reddit, Stocktwits, Binance, TAAPI, CoinDesk, CoinStats, BlockBeats, and Alternative.me response parsing;
- LLM provider registry, API-key environment mapping, quick/deep provider selection, model/endpoint validation, and provider capability flags;
- config precedence and secret redaction;
- checkpoint version/fingerprint checks;
- memory point-in-time and settlement rules;
- backtest holding-period and benchmark calculations.

### Workflow tests

Use mock data providers and a deterministic mock LLM to verify:

- analyst concurrency boundaries;
- bull/bear and risk turn ordering;
- correct state propagation;
- crypto Fundamentals inclusion;
- investment preferences and external reports reach intended stages;
- unavailable/withheld data remain distinguishable;
- quick/deep model tiers route to the configured providers;
- checkpoint resume starts at the correct stage;
- reports are emitted incrementally;
- final rating integrity and invalid-decision handling;
- memory settlement/reflection hooks execute at the correct workflow boundaries.

### Python/Rust parity tests

Use fixed vendor fixtures and deterministic LLM fixtures. Compare semantic state rather than natural-language text:

- selected analysts;
- vendor routing and fallback chains;
- which vendor calls are allowed or withheld;
- report presence/absence;
- debate/risk/tool round counts;
- quick/deep provider routing;
- trader/final action enums and rating integrity;
- availability of decision-level fields;
- checkpoint invalidation behavior;
- memory settlement visibility by analysis date;
- backtest holding-period outcomes;
- report artifact layout and HTML-generation toggle.

The Python implementation is the reference for existing v0.6 behavior. Where this design intentionally restores a missing Tomortec capability, the parity test records that as an approved Rust extension rather than treating the Python omission as expected behavior.

## 24. CI gates

The Rust workspace adds CI checks for:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

At least one Linux stable-Rust job is required initially. A minimum supported Rust version may be fixed after dependency selection; until then CI tracks stable Rust.

Python CI remains unchanged during the side-by-side migration.

## 25. Delivery phases

Implementation is divided into dependency-ordered phases while remaining one coherent port:

1. Workspace foundation, shared domain types, configuration, error model, rating model, and test harness.
2. Data-provider traits, router semantics, current stock/general vendors, crypto providers, and historical guards.
3. LLM abstraction, current provider families, quick/deep tier routing, research capability, prompts, and structured output.
4. Agent implementations and Tokio workflow orchestration.
5. Investment preferences, external reports, portfolio, memory settlement/reflection, checkpoint/resume, and rating/post-decision behavior.
6. CLI, incremental Markdown/HTML reporting, and backtesting.
7. Deterministic workflow/parity fixtures, CI, documentation, and final integration review.

Each phase must leave the Rust workspace compiling and its completed tests passing. Features are integrated behind stable interfaces rather than creating throwaway scaffolding that is later replaced.

## 26. Non-goals

The initial Rust port does not include:

- SMTP/email report delivery;
- automatic trade execution or exchange order placement;
- Freqtrade integration;
- UI/web frontend;
- PDF report generation;
- byte-for-byte reproduction of Python/LangChain/LangGraph internals;
- exact reproduction of the Python terminal UI;
- guaranteed identical natural-language prose across LLM providers;
- removal of Python before Rust parity gates pass.

## 27. Security and financial-safety boundaries

The application remains an analysis/research system, not an autonomous broker.

The Rust runtime must not place exchange orders. External reports are treated as untrusted content. Credentials are redacted from persisted output. Historical-data guards are enforced deterministically. Missing market evidence must not be converted into invented prices or ratios.

The final trade decision remains advisory output for a human or downstream caller to evaluate.

## 28. Acceptance boundary

This design is complete when the side-by-side Rust runtime can reproduce the existing Python core workflow and currently supported core provider/data/backtest behaviors with deterministic parity fixtures, includes the four missing Tomortec capabilities identified in the audit, and passes the Rust CI gates without weakening the existing Python implementation.

Making Rust the repository's sole/default runtime is explicitly outside this design's acceptance boundary and requires a separate migration decision after parity results are reviewed.
