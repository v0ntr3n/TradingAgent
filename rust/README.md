# TradingAgent — native Rust implementation

The `rust/` workspace is the native Rust implementation of the TradingAgent core. It lives beside the Python implementation so both runtimes can be verified independently.

## What is implemented

The Rust runtime includes the typed multi-agent workflow (analysts, bull/bear research, research manager, trader, risk analysts, portfolio manager), stock/crypto symbol handling, quick/deep LLM tiers, OpenAI-compatible/Qwen, Anthropic, Google, Azure and Bedrock provider adapters, structured trader output, investment preferences, caller-supplied external reports, portfolio context, checkpoint/resume, incremental Markdown reports, self-contained HTML output, decision memory, point-in-time-safe historical behavior, and analytical backtesting.

Crypto evidence supports Binance USD-M futures, TAAPI indicators, Alternative.me Fear & Greed, CoinDesk, CoinStats, BlockBeats and BTC dominance. Live/current-only sources are withheld during historical runs instead of leaking future information.

## Requirements

Rust 1.85 or newer is required.

```bash
cd rust
cargo build --workspace
```

The merge gate is:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Headless CLI

The binary is `trading-agent`. It never requires a TTY prompt.

```bash
cargo run -p trading-agent-cli -- \
  BTC-USD \
  --date 2026-10-06 \
  --analysts market,sentiment,news,fundamentals
```

Configuration precedence is:

`CLI flags > environment variables > optional JSON config file > defaults`.

Useful flags include:

```text
--config <file>
--quick-provider <provider>
--quick-model <model>
--quick-base-url <url>
--deep-provider <provider>
--deep-model <model>
--deep-base-url <url>
--output-language <language>
--max-debate-rounds <n>
--max-risk-rounds <n>
--preferences <preferences.json>
--external-report <file>        # repeatable
--portfolio <portfolio.json>
--results-dir <dir>
--checkpoint <checkpoint.json>
--memory <memory.json>
```

Crypto symbols such as `BTC`, `BTCUSDT`, `BTC-USDT`, and `BTC-USD` normalize to `BTC-USD`. Crypto runs keep the Fundamentals analyst enabled.

## LLM credentials

Provider credentials are read from environment variables and are not required on the command line:

```bash
OPENAI_API_KEY=...
DASHSCOPE_API_KEY=...
ANTHROPIC_API_KEY=...
GOOGLE_API_KEY=...
AZURE_OPENAI_API_KEY=...
OPENROUTER_API_KEY=...
GITEE_API_KEY=...
AWS_BEARER_TOKEN_BEDROCK=...
```

The quick and deep tiers may use different providers, models, and endpoints:

```bash
export TRADINGAGENTS_QUICK_THINK_PROVIDER=qwen
export TRADINGAGENTS_QUICK_THINK_LLM=qwen-plus
export TRADINGAGENTS_DEEP_THINK_PROVIDER=anthropic
export TRADINGAGENTS_DEEP_THINK_LLM=claude-sonnet-4-6
```

Other shared headless settings include:

```bash
TRADINGAGENTS_OUTPUT_LANGUAGE=English
TRADINGAGENTS_ANALYSTS=market,sentiment,news,fundamentals
TRADINGAGENTS_MAX_DEBATE_ROUNDS=1
TRADINGAGENTS_MAX_RISK_ROUNDS=1
TRADINGAGENTS_RESULTS_DIR=results
TRADINGAGENTS_CHECKPOINT_PATH=results/checkpoint.json
TRADINGAGENTS_MEMORY_LOG_PATH=results/memory.json
```

Optional crypto source keys:

```bash
TAAPI_API_KEY=...
COINDESK_API_KEY=...
COINSTATS_API_KEY=...
```

Binance, Alternative.me and BlockBeats paths used by the Rust crypto runtime do not require API keys.

The `TRADINGAGENTS_*_BASE_URL` variables used by the parity harness can override data endpoints for self-hosting/testing: `BINANCE`, `TAAPI`, `ALTERNATIVE_ME`, `BLOCKBEATS`, `COINDESK`, `COINSTATS`, `YAHOO`, and `POLYMARKET`.

## Optional input files

Investment preferences are JSON. They can capture spot/futures venue, long/short permission, risk style, holding horizon, leverage range, maximum acceptable loss, and free-form instructions.

```json
{
  "venue": "futures",
  "allow_long": true,
  "allow_short": true,
  "style": "aggressive",
  "holding_horizon": "1-3 days",
  "min_leverage": "25",
  "max_leverage": "100",
  "max_loss_pct": "50"
}
```

External reports are plain text/Markdown files. They are injected as explicitly untrusted evidence rather than instructions. `--external-report` may be repeated.

Portfolio input is JSON:

```json
{
  "cash": "10000",
  "currency": "USD",
  "positions": [
    {
      "symbol": "BTC-USD",
      "quantity": "1",
      "average_price": "60000"
    }
  ]
}
```

## Output and persistence

By default a run writes under:

```text
results/<SYMBOL>/<YYYY-MM-DD>/
├── run_metadata.json
├── reports/
├── complete.md
└── complete.html
```

Run metadata is redacted before persistence. Checkpoints are versioned and resume only when the behavioral run signature matches. A stale signature is discarded; an unsupported checkpoint schema is rejected before agents run.

Decision memory records completed decisions as pending, settles outcomes later through the memory abstraction, and exposes only lessons whose resolution date was known by the historical analysis date.

## Backtesting

The Rust backtest runtime reuses the same workflow contracts and point-in-time policy. It sorts/deduplicates cells, isolates unavailable dates, measures configured holding-window return and benchmark alpha, and reports rating-level hit rate/mean alpha.

It is an analytical backtest, not a simulated brokerage. It does not invent fills or carry a synthetic cash/position ledger between cells.

## Parity tests

Deterministic fixtures cover:

- stock runs;
- live crypto runs;
- historical crypto runs with current-only data withheld;
- analyst/report state fields;
- quick/deep tier routing;
- crypto Fundamentals as the approved extension;
- structured entry, support, resistance, take-profit and stop-loss values.

Run all parity tests with the normal workspace command:

```bash
cargo test --workspace
```
