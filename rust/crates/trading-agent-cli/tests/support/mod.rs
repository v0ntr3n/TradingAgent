use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    thread,
    time::Duration,
};

use clap::Parser;
use chrono::{Days, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use tempfile::TempDir;
use tiny_http::{Header, Response, Server};
use trading_agent_cli::{CliArgs, EnvSource, resolve_config, run_from_args};
use trading_agent_core::{AssetType, workflow::RunResult};

#[derive(Clone, Debug, Deserialize)]
pub struct ParityFixture {
    pub name: String,
    pub symbol: String,
    pub date: String,
    pub expected_asset_type: String,
    pub expected_rating: String,
    pub expected_reports: Vec<String>,
    pub expected_vendor_paths: Vec<String>,
    pub expected_quick_labels: Vec<String>,
    pub expected_deep_labels: Vec<String>,
    pub expected_search_calls: usize,
    pub report_status_contains: String,
    pub approved_extensions: Vec<String>,
}

#[derive(Default)]
pub struct FixtureEnv {
    values: HashMap<String, String>,
}

impl FixtureEnv {
    pub fn set(mut self, key: &str, value: impl Into<String>) -> Self {
        self.values.insert(key.into(), value.into());
        self
    }
}

impl EnvSource for FixtureEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
}

#[derive(Clone, Debug)]
pub struct RequestRecord {
    pub url: String,
    pub label: Option<String>,
}

pub struct FixtureServer {
    base_url: String,
    records: Arc<Mutex<Vec<RequestRecord>>>,
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl FixtureServer {
    pub fn start() -> Self {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());
        let records = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let records_thread = records.clone();
        let stop_thread = stop.clone();

        let join = thread::spawn(move || {
            while !stop_thread.load(Ordering::SeqCst) {
                let Some(mut request) = server.recv_timeout(Duration::from_millis(50)).unwrap() else {
                    continue;
                };
                let url = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                let (payload, label) = route(&url, &body);
                records_thread.lock().unwrap().push(RequestRecord { url, label });
                let response = Response::from_string(payload.to_string()).with_header(
                    Header::from_bytes("content-type", "application/json").unwrap(),
                );
                request.respond(response).unwrap();
            }
        });

        Self { base_url, records, stop, join: Some(join) }
    }

    pub fn base(&self, prefix: &str) -> String {
        format!("{}{}", self.base_url, prefix)
    }

    pub fn records(&self) -> Vec<RequestRecord> {
        self.records.lock().unwrap().clone()
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            join.join().unwrap();
        }
    }
}

fn route(url: &str, body: &str) -> (Value, Option<String>) {
    if url.contains("/chat/completions") {
        let value: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
        let label = classify_llm(&value);
        let prompt = value.get("messages")
            .and_then(Value::as_array)
            .map(|messages| messages.iter()
                .filter_map(|message| message.get("content").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"))
            .unwrap_or_default();
        let content = match label.as_str() {
            "trader" => r#"{"action":"buy","reasoning":"fixture trader","entry_price":"100","support":"95","resistance":"120","take_profit":"115","stop_loss":"90","position_sizing":{"description":"fixture","percent_of_portfolio":"5"}}"#.to_string(),
            "portfolio_manager" => "**Rating**: Buy\n\nfixture final decision".into(),
            "search" => "fixture search evidence".into(),
            other if matches!(other, "market" | "sentiment" | "news" | "fundamentals") => {
                let status = if prompt.contains("EVIDENCE STATUS: WITHHELD_HISTORICAL") {
                    "withheld"
                } else {
                    "available"
                };
                format!("{other} report {status}")
            }
            other => format!("{other} fixture output"),
        };
        return (json!({"choices":[{"message":{"content":content}}]}), Some(label));
    }

    let payload = if url.starts_with("/binance/fapi/v1/klines") {
        json!([[1,"100","110","90","105","10"],[2,"105","115","100","110","12"]])
    } else if url.starts_with("/binance/fapi/v1/depth") {
        json!({"bids":[["109","1"]],"asks":[["110","1"]]})
    } else if url.starts_with("/binance/fapi/v1/ticker/24hr") {
        json!({"lastPrice":"110","priceChangePercent":"5","highPrice":"115","lowPrice":"95","quoteVolume":"1000000"})
    } else if url.starts_with("/binance/futures/data/") {
        json!([{"longShortRatio":"1.2","buySellRatio":"1.1"}])
    } else if url.starts_with("/taapi/bulk") {
        json!({"data":[{"id":"rsi","result":{"value":55}}]})
    } else if url.starts_with("/alternative/fng/") {
        json!({"data":[{"value":"55","value_classification":"Neutral"}]})
    } else if url.starts_with("/blockbeats/v1/open-api/open-flash") {
        json!({"data":{"data":[{"title":"BlockBeats fixture","create_time":"2026-10-06","content":"fixture news"}]}})
    } else if url.starts_with("/coindesk/news/v1/article/list") {
        json!({"Data":[{"TITLE":"CoinDesk fixture","SENTIMENT":"POSITIVE","BODY":"fixture news","URL":"https://example.invalid/coindesk"}]})
    } else if url.starts_with("/coinstats/news/type/latest") {
        json!({"result":[{"title":"CoinStats fixture","source":"fixture","description":"fixture news"}]})
    } else if url.starts_with("/coinstats/insights/btc-dominance") {
        json!({"data":[[1,52.1]]})
    } else if url.starts_with("/yahoo/v8/finance/chart/AAPL") {
        let timestamp = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        json!({"chart":{"result":[{"timestamp":[timestamp],"indicators":{"quote":[{"close":[210.0]}]}}]}})
    } else if url.starts_with("/polymarket/markets") {
        json!({"markets":[{"question":"fixture market"}]})
    } else {
        json!({"error":format!("unhandled fixture route: {url}")})
    };
    (payload, None)
}

fn classify_llm(value: &Value) -> String {
    if value.get("enable_search").and_then(Value::as_bool) == Some(true) {
        return "search".into();
    }
    let prompt = value.get("messages")
        .and_then(Value::as_array)
        .map(|messages| messages.iter()
            .filter_map(|message| message.get("content").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"))
        .unwrap_or_default();

    for (needle, label) in [
        ("TradingAgent market analyst", "market"),
        ("TradingAgent sentiment analyst", "sentiment"),
        ("TradingAgent news analyst", "news"),
        ("crypto fundamentals analyst", "fundamentals"),
        ("TradingAgent fundamentals analyst", "fundamentals"),
        ("Act as the bull researcher", "bull"),
        ("Act as the bear researcher", "bear"),
        ("Act as Research Manager", "research_manager"),
        ("Act as the trader", "trader"),
        ("Act as the aggressive risk analyst", "risk_aggressive"),
        ("Act as the neutral risk analyst", "risk_neutral"),
        ("Act as the conservative risk analyst", "risk_conservative"),
        ("Act as Portfolio Manager", "portfolio_manager"),
    ] {
        if prompt.contains(needle) {
            return label.into();
        }
    }
    "other".into()
}

pub async fn run_fixture(fixture: &ParityFixture) -> (RunResult, Vec<RequestRecord>) {
    let server = FixtureServer::start();
    let temp = TempDir::new().unwrap();

    let env = FixtureEnv::default()
        .set("OPENAI_API_KEY", "fixture-key")
        .set("TAAPI_API_KEY", "fixture-taapi")
        .set("COINDESK_API_KEY", "fixture-coindesk")
        .set("COINSTATS_API_KEY", "fixture-coinstats")
        .set("TRADINGAGENTS_BINANCE_BASE_URL", server.base("/binance"))
        .set("TRADINGAGENTS_TAAPI_BASE_URL", server.base("/taapi"))
        .set("TRADINGAGENTS_ALTERNATIVE_ME_BASE_URL", server.base("/alternative"))
        .set("TRADINGAGENTS_BLOCKBEATS_BASE_URL", server.base("/blockbeats"))
        .set("TRADINGAGENTS_COINDESK_BASE_URL", server.base("/coindesk"))
        .set("TRADINGAGENTS_COINSTATS_BASE_URL", server.base("/coinstats"))
        .set("TRADINGAGENTS_YAHOO_BASE_URL", server.base("/yahoo"))
        .set("TRADINGAGENTS_POLYMARKET_BASE_URL", server.base("/polymarket"));

    let today = Utc::now().date_naive();
    let run_date = match fixture.date.as_str() {
        "current" => today,
        "historical-5d" => today.checked_sub_days(Days::new(5)).unwrap(),
        other => chrono::NaiveDate::parse_from_str(other, "%Y-%m-%d").unwrap(),
    };

    let args = CliArgs::try_parse_from(vec![
        "trading-agent".into(),
        fixture.symbol.clone(),
        "--date".into(),
        run_date.format("%Y-%m-%d").to_string(),
        "--analysts".into(),
        "market,sentiment,news,fundamentals".into(),
        "--quick-provider".into(),
        "openai_compatible".into(),
        "--quick-model".into(),
        "fixture-quick".into(),
        "--quick-base-url".into(),
        server.base("/quick/v1"),
        "--deep-provider".into(),
        "openai_compatible".into(),
        "--deep-model".into(),
        "fixture-deep".into(),
        "--deep-base-url".into(),
        server.base("/deep/v1"),
        "--results-dir".into(),
        temp.path().display().to_string(),
    ]).unwrap();

    let resolved = resolve_config(&args, &env).unwrap();
    assert_eq!(resolved.data_endpoints.binance, server.base("/binance"));
    let result = run_from_args(args, &env).await.unwrap();
    let records = server.records();
    (result, records)
}

pub fn assert_fixture(fixture: &ParityFixture, result: &RunResult, records: &[RequestRecord]) {
    let asset = match result.state.asset_type {
        AssetType::Stock => "stock",
        AssetType::Crypto => "crypto",
    };
    assert_eq!(asset, fixture.expected_asset_type, "{}", fixture.name);
    assert_eq!(format!("{:?}", result.rating).to_ascii_lowercase(), fixture.expected_rating);

    for report in &fixture.expected_reports {
        let present = match report.as_str() {
            "market" => result.state.market_report.as_ref(),
            "sentiment" => result.state.sentiment_report.as_ref(),
            "news" => result.state.news_report.as_ref(),
            "fundamentals" => result.state.fundamentals_report.as_ref(),
            other => panic!("unknown report {other}"),
        };
        let text = present.unwrap_or_else(|| panic!("missing {report} report"));
        assert!(
            text.to_ascii_lowercase().contains(&fixture.report_status_contains),
            "{} report status mismatch: {}",
            report,
            text,
        );
    }

    let proposal = result.state.trader_proposal.as_ref().unwrap();
    assert_eq!(proposal.entry_price.unwrap().to_string(), "100");
    assert_eq!(proposal.support.unwrap().to_string(), "95");
    assert_eq!(proposal.resistance.unwrap().to_string(), "120");
    assert_eq!(proposal.take_profit.unwrap().to_string(), "115");
    assert_eq!(proposal.stop_loss.unwrap().to_string(), "90");

    for path in &fixture.expected_vendor_paths {
        assert!(records.iter().any(|record| record.url.starts_with(path)), "missing fixture vendor call {path}");
    }
    if fixture.expected_vendor_paths.is_empty() {
        assert!(records.iter().all(|record| record.url.starts_with("/quick/") || record.url.starts_with("/deep/")));
    }

    let mut quick = records.iter()
        .filter(|record| record.url.starts_with("/quick/"))
        .filter_map(|record| record.label.clone())
        .collect::<Vec<_>>();
    let mut expected_quick = fixture.expected_quick_labels.clone();
    quick.sort();
    expected_quick.sort();
    assert_eq!(quick, expected_quick, "quick-tier routing mismatch");

    let mut deep = records.iter()
        .filter(|record| record.url.starts_with("/deep/"))
        .filter_map(|record| record.label.clone())
        .collect::<Vec<_>>();
    let mut expected_deep = fixture.expected_deep_labels.clone();
    deep.sort();
    expected_deep.sort();
    assert_eq!(deep, expected_deep, "deep-tier routing mismatch");

    assert_eq!(
        records.iter().filter(|record| record.label.as_deref() == Some("search")).count(),
        fixture.expected_search_calls,
    );

    if fixture.expected_asset_type == "crypto" {
        assert!(fixture.approved_extensions.iter().any(|item| item == "crypto_fundamentals"));
        assert!(result.state.fundamentals_report.is_some());
    }
}
