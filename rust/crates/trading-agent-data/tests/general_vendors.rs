use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::{Value, json};
use trading_agent_core::Symbol;
use trading_agent_data::{
    DataError, DataRequest, DataStatus, FundamentalsSource, HttpRequest, HttpTransport,
    MacroSource, MarketDataSource, SentimentSource,
    general::{
        AlphaVantageProvider, FredProvider, PolymarketProvider, SecEdgarProvider, SocialProvider,
        YahooProvider,
    },
};

#[derive(Clone)]
struct StaticTransport {
    response: Arc<Mutex<Option<Result<Value, DataError>>>>,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl StaticTransport {
    fn ok(value: Value) -> Self {
        Self {
            response: Arc::new(Mutex::new(Some(Ok(value)))),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn fail(vendor: &str, message: &str) -> Self {
        Self {
            response: Arc::new(Mutex::new(Some(Err(DataError::Vendor {
                vendor: vendor.into(),
                message: message.into(),
            })))),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

#[async_trait]
impl HttpTransport for StaticTransport {
    async fn execute(&self, request: HttpRequest) -> Result<Value, DataError> {
        self.requests.lock().unwrap().push(request);
        self.response.lock().unwrap().take().unwrap()
    }
}

fn request(symbol: &str, date: &str) -> DataRequest {
    DataRequest {
        symbol: Symbol::parse(symbol).unwrap(),
        as_of: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
    }
}

#[tokio::test]
async fn yahoo_rejects_stale_daily_market_data() {
    let transport = StaticTransport::ok(json!({
        "chart": {"result": [{
            "timestamp": [1761868800],
            "indicators": {"quote": [{"close": [210.5]}]}
        }]}
    }));
    let provider = YahooProvider::new(Arc::new(transport), "https://query1.finance.yahoo.com");
    let status = provider
        .market_snapshot(&request("AAPL", "2026-11-10"))
        .await
        .unwrap();
    assert!(matches!(status, DataStatus::Unavailable { reason, .. } if reason.contains("stale")));
}

#[tokio::test]
async fn sec_edgar_excludes_filings_not_known_by_as_of_date() {
    let transport = StaticTransport::ok(json!({
        "filings": [
            {"filed_at": "2026-01-15", "form": "10-K", "summary": "known filing"},
            {"filed_at": "2026-03-15", "form": "10-Q", "summary": "future filing"}
        ]
    }));
    let provider = SecEdgarProvider::new(Arc::new(transport), "https://data.sec.gov");
    let status = provider
        .fundamentals(&request("AAPL", "2026-02-01"))
        .await
        .unwrap();
    let DataStatus::Available(snapshot) = status else {
        panic!("expected available fundamentals")
    };
    assert!(snapshot.summary.contains("known filing"));
    assert!(!snapshot.summary.contains("future filing"));
}

#[tokio::test]
async fn alpha_vantage_api_errors_remain_typed_vendor_failures() {
    let transport = StaticTransport::ok(json!({"Error Message": "Invalid API call"}));
    let provider = AlphaVantageProvider::new(
        Arc::new(transport),
        "https://www.alphavantage.co",
        "secret-key",
    );
    let error = provider
        .market_snapshot(&request("AAPL", "2026-10-06"))
        .await
        .unwrap_err();
    assert!(
        matches!(error, DataError::Vendor { vendor, message } if vendor == "alpha_vantage" && message.contains("Invalid API call"))
    );
}

#[tokio::test]
async fn fred_macro_request_is_bounded_by_as_of_date() {
    let transport = StaticTransport::ok(json!({
        "observations": [{"date": "2026-09-01", "value": "4.25"}]
    }));
    let inspect = transport.clone();
    let provider = FredProvider::new(
        Arc::new(transport),
        "https://api.stlouisfed.org",
        "fred-key",
    );
    let status = provider
        .macro_snapshot(&request("AAPL", "2026-10-06"))
        .await
        .unwrap();
    assert!(matches!(status, DataStatus::Available(_)));
    let requests = inspect.requests.lock().unwrap();
    assert!(
        requests[0]
            .query
            .iter()
            .any(|(k, v)| k == "observation_end" && v == "2026-10-06")
    );
}

#[tokio::test]
async fn current_only_polymarket_is_withheld_before_http_for_historical_runs() {
    let transport = StaticTransport::ok(json!({"markets": []}));
    let inspect = transport.clone();
    let provider = PolymarketProvider::new(
        Arc::new(transport),
        "https://gamma-api.polymarket.com",
        NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(),
    );
    let status = provider
        .sentiment(&request("AAPL", "2026-10-05"))
        .await
        .unwrap();
    assert!(matches!(status, DataStatus::WithheldHistorical { .. }));
    assert_eq!(inspect.request_count(), 0);
}

#[tokio::test]
async fn social_transport_failure_degrades_to_unavailable_context() {
    let transport = StaticTransport::fail("http", "upstream unavailable");
    let provider = SocialProvider::new(
        Arc::new(transport),
        "https://social.example",
        NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(),
    );
    let status = provider
        .sentiment(&request("AAPL", "2026-10-06"))
        .await
        .unwrap();
    assert!(
        matches!(status, DataStatus::Unavailable { source, reason } if source == "social" && reason.contains("unavailable"))
    );
}
