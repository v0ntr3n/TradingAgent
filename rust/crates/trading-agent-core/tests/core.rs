use chrono::NaiveDate;
use trading_agent_core::{AgentState, AssetType, ModelTierConfig, RunConfig, Symbol, SCHEMA_VERSION};

#[test]
fn normalizes_crypto_symbols() {
    for raw in ["BTC", "BTCUSD", "BTC-USDT", "BTC-USD"] {
        let symbol = Symbol::parse(raw).expect("crypto symbol should parse");
        assert_eq!(symbol.asset_type(), AssetType::Crypto);
        assert_eq!(symbol.as_str(), "BTC-USD");
    }
}

#[test]
fn preserves_stock_exchange_suffixes() {
    let hk = Symbol::parse("0700.HK").unwrap();
    assert_eq!(hk.asset_type(), AssetType::Stock);
    assert_eq!(hk.as_str(), "0700.HK");

    let us = Symbol::parse("aapl").unwrap();
    assert_eq!(us.asset_type(), AssetType::Stock);
    assert_eq!(us.as_str(), "AAPL");
}

#[test]
fn rejects_invalid_symbols() {
    for raw in ["", "   ", "../BTC", "BTC/USD", "$BTC"] {
        assert!(Symbol::parse(raw).is_err(), "{raw:?} must be rejected");
    }
}

fn config(api_key: &str, model: &str) -> RunConfig {
    RunConfig {
        quick: ModelTierConfig {
            provider: "openai_compatible".into(),
            model: model.into(),
            base_url: Some("https://example.test/v1".into()),
            api_key: Some(api_key.into()),
        },
        deep: ModelTierConfig {
            provider: "openai_compatible".into(),
            model: "deep-model".into(),
            base_url: Some("https://deep.example.test/v1".into()),
            api_key: Some(api_key.into()),
        },
        output_language: "English".into(),
        analysts: vec!["market".into(), "news".into()],
        max_debate_rounds: 1,
        max_risk_rounds: 1,
    }
}

#[test]
fn config_fingerprint_tracks_behavior_but_not_secret_values() {
    let a = config("secret-a", "quick-a");
    let b = config("secret-b", "quick-a");
    let c = config("secret-a", "quick-b");

    assert_eq!(a.fingerprint(), b.fingerprint(), "rotating a credential must not invalidate a run");
    assert_ne!(a.fingerprint(), c.fingerprint(), "changing the model must invalidate a run");
    assert!(!a.fingerprint().contains("secret-a"));
    assert!(!a.safe_metadata().to_string().contains("secret-a"));
}

#[test]
fn base_state_is_schema_versioned_and_serializable() {
    let symbol = Symbol::parse("BTC-USD").unwrap();
    let state = AgentState::new(symbol, NaiveDate::from_ymd_opt(2026, 10, 6).unwrap());
    assert_eq!(state.schema_version, SCHEMA_VERSION);
    assert_eq!(state.asset_type, AssetType::Crypto);
    let encoded = serde_json::to_string(&state).unwrap();
    assert!(encoded.contains("2026-10-06"));
}
