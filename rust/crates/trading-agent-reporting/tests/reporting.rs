use std::fs;

use chrono::NaiveDate;
use serde_json::json;
use tempfile::tempdir;
use trading_agent_core::{
    AgentState, ModelTierConfig, RunConfig, Symbol,
    agents::{AgentEvidence, Evidence},
    checkpoint::RunSignature,
    events::{RunEvent, WorkflowStage},
    workflow::{FinalRating, RunInput, RunResult},
};
use trading_agent_reporting::ReportWriter;

fn input_with_secret_url() -> RunInput {
    RunInput {
        state: AgentState::new(
            Symbol::parse("AAPL").unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(),
        ),
        evidence: AgentEvidence {
            market: Evidence::Available("market".into()),
            sentiment: Evidence::Unavailable {
                source: "social".into(),
                reason: "not configured".into(),
            },
            news: Evidence::Available("news".into()),
            fundamentals: Evidence::Available("fundamentals".into()),
        },
        config: RunConfig {
            quick: ModelTierConfig {
                provider: "openai_compatible".into(),
                model: "quick".into(),
                base_url: Some(
                    "https://user:pass@example.com/v1?api_key=query-secret&region=us".into(),
                ),
                api_key: Some("header-secret".into()),
            },
            deep: ModelTierConfig {
                provider: "anthropic".into(),
                model: "deep".into(),
                base_url: None,
                api_key: None,
            },
            output_language: "English".into(),
            analysts: vec!["market".into(), "news".into(), "fundamentals".into()],
            max_debate_rounds: 1,
            max_risk_rounds: 1,
        },
    }
}

#[test]
fn report_writer_persists_safe_metadata_incremental_sections_and_self_contained_outputs() {
    let temp = tempdir().unwrap();
    let writer = ReportWriter::new(temp.path()).unwrap();
    let input = input_with_secret_url();
    let signature = RunSignature::from_inputs(&input);
    let metadata = RunSignature::safe_metadata(&input);

    writer
        .write_event(&RunEvent::Started {
            signature,
            metadata,
        })
        .unwrap();
    let metadata_text = fs::read_to_string(temp.path().join("run_metadata.json")).unwrap();
    for secret in ["query-secret", "header-secret", "user:pass"] {
        assert!(!metadata_text.contains(secret), "persisted metadata leaked {secret}");
    }
    assert!(metadata_text.contains("<redacted>"));

    writer
        .write_event(&RunEvent::SectionCompleted {
            stage: WorkflowStage::Analysts,
            section: "market".into(),
            content: "market section".into(),
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join("reports/market.md")).unwrap(),
        "market section\n"
    );

    let mut state = input.state;
    state.market_report = Some("market section".into());
    state.news_report = Some("news section".into());
    state.fundamentals_report = Some("fundamentals section".into());
    state.research_plan = Some("research section".into());
    state.trader_plan = Some("trader section".into());
    state.final_decision = Some("**Rating**: Buy\n\nportfolio section".into());
    let result = RunResult {
        state,
        rating: FinalRating::Buy,
    };

    writer.write_complete(&result).unwrap();

    let markdown = fs::read_to_string(temp.path().join("complete.md")).unwrap();
    assert!(markdown.contains("# TradingAgent Report"));
    assert!(markdown.contains("## Market"));
    assert!(markdown.contains("market section"));
    assert!(markdown.contains("## Final Decision"));
    assert!(markdown.contains("Buy"));

    let html = fs::read_to_string(temp.path().join("complete.html")).unwrap();
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("<style>"));
    assert!(!html.contains("<link"));
    assert!(!html.contains("<script src="));
}

#[test]
fn report_writer_rejects_unsafe_section_names() {
    let temp = tempdir().unwrap();
    let writer = ReportWriter::new(temp.path()).unwrap();
    let event = RunEvent::SectionCompleted {
        stage: WorkflowStage::Analysts,
        section: "../secret".into(),
        content: json!({"value": 1}).to_string(),
    };
    assert!(writer.write_event(&event).is_err());
}
