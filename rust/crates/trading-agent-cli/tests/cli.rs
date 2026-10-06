use std::{collections::HashMap, fs};

use clap::Parser;
use predicates::prelude::*;
use tempfile::tempdir;
use trading_agent_cli::{CliArgs, EnvSource, build_run_input, resolve_config};
use trading_agent_core::AssetType;

#[derive(Default)]
struct MapEnv(HashMap<String, String>);

impl MapEnv {
    fn with(mut self, key: &str, value: &str) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }
}

impl EnvSource for MapEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

#[test]
fn headless_parse_detects_crypto_and_keeps_fundamentals() {
    let args = CliArgs::try_parse_from([
        "trading-agent",
        "BTCUSDT",
        "--date",
        "2026-10-06",
        "--analysts",
        "market,fundamentals",
    ])
    .unwrap();
    let resolved = resolve_config(&args, &MapEnv::default()).unwrap();

    assert_eq!(resolved.symbol.asset_type(), AssetType::Crypto);
    assert_eq!(resolved.symbol.as_str(), "BTC-USD");
    assert_eq!(resolved.run.analysts, vec!["market", "fundamentals"]);
}

#[test]
fn invalid_symbol_and_date_fail_before_runtime() {
    let bad_symbol =
        CliArgs::try_parse_from(["trading-agent", "../BTC", "--date", "2026-10-06"]).unwrap();
    assert!(resolve_config(&bad_symbol, &MapEnv::default()).is_err());

    let bad_date =
        CliArgs::try_parse_from(["trading-agent", "AAPL", "--date", "06-10-2026"]).unwrap();
    assert!(resolve_config(&bad_date, &MapEnv::default()).is_err());
}

#[test]
fn precedence_is_cli_over_env_over_file_over_defaults() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.json");
    fs::write(
        &path,
        r#"{
      "quick_provider":"google",
      "quick_model":"file-quick",
      "deep_provider":"google",
      "deep_model":"file-deep",
      "output_language":"French",
      "max_debate_rounds":2,
      "max_risk_rounds":2
    }"#,
    )
    .unwrap();

    let env = MapEnv::default()
        .with("TRADINGAGENTS_QUICK_THINK_PROVIDER", "anthropic")
        .with("TRADINGAGENTS_QUICK_THINK_LLM", "env-quick")
        .with("TRADINGAGENTS_DEEP_THINK_PROVIDER", "anthropic")
        .with("TRADINGAGENTS_OUTPUT_LANGUAGE", "Thai");

    let args = CliArgs::try_parse_from([
        "trading-agent",
        "AAPL",
        "--date",
        "2026-10-06",
        "--config",
        path.to_str().unwrap(),
        "--quick-provider",
        "openai",
        "--quick-model",
        "cli-quick",
    ])
    .unwrap();
    let resolved = resolve_config(&args, &env).unwrap();

    assert_eq!(resolved.run.quick.provider, "openai");
    assert_eq!(resolved.run.quick.model, "cli-quick");
    assert_eq!(resolved.run.deep.provider, "anthropic");
    assert_eq!(resolved.run.deep.model, "file-deep");
    assert_eq!(resolved.run.output_language, "Thai");
    assert_eq!(resolved.run.max_debate_rounds, 2);
}

#[test]
fn diagnostics_never_include_provider_secrets_or_url_credentials() {
    let env = MapEnv::default()
        .with("OPENAI_API_KEY", "header-secret")
        .with(
            "TRADINGAGENTS_QUICK_THINK_BACKEND_URL",
            "https://user:pass@example.com/v1?api_key=query-secret&region=us",
        );
    let args = CliArgs::try_parse_from([
        "trading-agent",
        "AAPL",
        "--date",
        "2026-10-06",
        "--quick-provider",
        "openai",
    ])
    .unwrap();

    let resolved = resolve_config(&args, &env).unwrap();
    let summary = resolved.safe_summary();
    for secret in ["header-secret", "query-secret", "user:pass"] {
        assert!(
            !summary.contains(secret),
            "safe diagnostics leaked {secret}"
        );
    }
    assert!(summary.contains("<redacted>"));
}

#[test]
fn preferences_external_reports_and_portfolio_files_reach_run_input() {
    let temp = tempdir().unwrap();
    let preferences = temp.path().join("preferences.json");
    let external = temp.path().join("outside.md");
    let portfolio = temp.path().join("portfolio.json");

    fs::write(
        &preferences,
        r#"{
      "venue":"futures","allow_long":true,"allow_short":true,
      "style":"aggressive","holding_horizon":"1-3 days",
      "min_leverage":"25","max_leverage":"100","max_loss_pct":"50"
    }"#,
    )
    .unwrap();
    fs::write(&external, "outside research").unwrap();
    fs::write(
        &portfolio,
        r#"{
      "cash":"10000","currency":"USD",
      "positions":[{"symbol":"BTC-USD","quantity":"1","average_price":"60000"}]
    }"#,
    )
    .unwrap();

    let args = CliArgs::try_parse_from([
        "trading-agent",
        "BTC-USD",
        "--date",
        "2026-10-06",
        "--preferences",
        preferences.to_str().unwrap(),
        "--external-report",
        external.to_str().unwrap(),
        "--portfolio",
        portfolio.to_str().unwrap(),
    ])
    .unwrap();
    let resolved = resolve_config(&args, &MapEnv::default()).unwrap();
    let input = build_run_input(&args, &resolved).unwrap();

    assert_eq!(
        input
            .state
            .investment_preferences
            .as_ref()
            .unwrap()
            .holding_horizon
            .as_deref(),
        Some("1-3 days")
    );
    assert_eq!(input.state.external_reports.len(), 1);
    assert_eq!(input.state.external_reports[0].content, "outside research");
    assert_eq!(
        input.state.portfolio.as_ref().unwrap().positions[0]
            .symbol
            .as_str(),
        "BTC-USD"
    );
}

#[test]
fn independent_quick_and_deep_tier_flags_are_preserved() {
    let args = CliArgs::try_parse_from([
        "trading-agent",
        "AAPL",
        "--date",
        "2026-10-06",
        "--quick-provider",
        "qwen",
        "--quick-model",
        "qwen-plus",
        "--quick-base-url",
        "https://quick.example/v1",
        "--deep-provider",
        "anthropic",
        "--deep-model",
        "claude-deep",
        "--deep-base-url",
        "https://deep.example",
    ])
    .unwrap();
    let resolved = resolve_config(&args, &MapEnv::default()).unwrap();

    assert_eq!(resolved.run.quick.provider, "qwen");
    assert_eq!(resolved.run.quick.model, "qwen-plus");
    assert_eq!(
        resolved.run.quick.base_url.as_deref(),
        Some("https://quick.example/v1")
    );
    assert_eq!(resolved.run.deep.provider, "anthropic");
    assert_eq!(resolved.run.deep.model, "claude-deep");
    assert_eq!(
        resolved.run.deep.base_url.as_deref(),
        Some("https://deep.example")
    );
}

#[test]
fn binary_help_is_noninteractive_and_does_not_require_a_tty() {
    let mut cmd = assert_cmd::Command::cargo_bin("trading-agent").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("TradingAgent"));
}
