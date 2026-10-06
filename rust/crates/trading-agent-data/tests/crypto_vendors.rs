use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use chrono_tz::Asia::Bangkok;
use serde_json::{Value, json};
use trading_agent_core::Symbol;
use trading_agent_data::crypto::{
    AlternativeMeClient, BinanceClient, BlockBeatsClient, CoinDeskClient, CoinStatsClient,
    TaapiClient, binance_symbol, taapi_symbol,
};
use trading_agent_data::{
    DataError, DataRequest, DataStatus, HttpMethod, HttpRequest, HttpTransport,
};

#[derive(Default)]
struct MockTransport {
    requests: Mutex<Vec<HttpRequest>>,
    responses: Mutex<VecDeque<Value>>,
}

impl MockTransport {
    fn with_responses(responses: impl IntoIterator<Item = Value>) -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(responses.into_iter().collect()),
        })
    }

    fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl HttpTransport for MockTransport {
    async fn execute(&self, request: HttpRequest) -> Result<Value, DataError> {
        self.requests.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| DataError::Vendor {
                vendor: "mock".into(),
                message: "no response queued".into(),
            })
    }
}

fn request(symbol: &str, y: i32, m: u32, d: u32) -> DataRequest {
    DataRequest {
        symbol: Symbol::parse(symbol).unwrap(),
        as_of: chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap(),
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 6, 7, 0, 0).unwrap()
}

fn query<'a>(req: &'a HttpRequest, key: &str) -> Option<&'a str> {
    req.query
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn header<'a>(req: &'a HttpRequest, key: &str) -> Option<&'a str> {
    req.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

#[test]
fn crypto_vendor_symbols_use_expected_quotes() {
    let btc = Symbol::parse("BTC-USD").unwrap();
    assert_eq!(binance_symbol(&btc), "BTCUSDT");
    assert_eq!(taapi_symbol(&btc), "BTC/USDT");
}

#[tokio::test]
async fn binance_requests_all_futures_microstructure_endpoints() {
    let transport = MockTransport::with_responses([
        json!([[1_791_264_600_000i64, "100", "110", "90", "105", "12"]]),
        json!({"bids":[["104","2"]],"asks":[["106","3"]]}),
        json!({"lastPrice":"105","priceChangePercent":"2.5","highPrice":"111","lowPrice":"89","quoteVolume":"123456"}),
        json!([{"timestamp":1_791_264_600_000i64,"longShortRatio":"1.2"}]),
        json!([{"timestamp":1_791_264_600_000i64,"longShortRatio":"1.3"}]),
        json!([{"timestamp":1_791_264_600_000i64,"longShortRatio":"1.1"}]),
        json!([{"timestamp":1_791_264_600_000i64,"buySellRatio":"1.4"}]),
    ]);
    let client = BinanceClient::new(transport.clone(), "https://binance.test", now(), Bangkok);

    let result = client
        .market_snapshot(&request("BTC-USD", 2026, 10, 6), "15m")
        .await
        .unwrap();
    let DataStatus::Available(snapshot) = result else {
        panic!("expected live snapshot")
    };
    assert!(snapshot.summary.contains("Last: 105"));
    assert!(snapshot.summary.contains("Best bid: 104"));
    assert!(snapshot.summary.contains("1.4"));

    let requests = transport.requests();
    assert_eq!(requests.len(), 7);
    let paths: Vec<_> = requests.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/fapi/v1/klines",
            "/fapi/v1/depth",
            "/fapi/v1/ticker/24hr",
            "/futures/data/topLongShortPositionRatio",
            "/futures/data/topLongShortAccountRatio",
            "/futures/data/globalLongShortAccountRatio",
            "/futures/data/takerlongshortRatio",
        ]
    );
    assert!(
        requests
            .iter()
            .all(|r| query(r, "symbol") == Some("BTCUSDT"))
    );
    assert_eq!(query(&requests[0], "interval"), Some("15m"));
    assert_eq!(query(&requests[3], "period"), Some("15m"));
}

#[tokio::test]
async fn historical_binance_is_withheld_before_transport_execution() {
    let transport = MockTransport::with_responses([]);
    let client = BinanceClient::new(transport.clone(), "https://binance.test", now(), Bangkok);

    let result = client
        .market_snapshot(&request("BTC-USD", 2026, 10, 5), "15m")
        .await
        .unwrap();
    assert!(matches!(result, DataStatus::WithheldHistorical { .. }));
    assert!(transport.requests().is_empty());
}

#[tokio::test]
async fn taapi_uses_bulk_constructs_and_bearer_auth() {
    let transport =
        MockTransport::with_responses([json!({"data":[{"id":"rsi","result":{"value":55.0}}]})]);
    let client = TaapiClient::new(
        transport.clone(),
        "https://taapi.test",
        "secret-key",
        now(),
        Bangkok,
    );

    let result = client
        .indicators(&request("ETH-USD", 2026, 10, 6), "1h")
        .await
        .unwrap();
    assert!(matches!(result, DataStatus::Available(_)));
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    let req = &requests[0];
    assert_eq!(req.method, HttpMethod::Post);
    assert_eq!(req.path, "/bulk");
    assert_eq!(header(req, "Authorization"), Some("Bearer secret-key"));
    let body = req.body.as_ref().unwrap();
    assert_eq!(body["constructs"][0]["symbol"], "ETH/USDT");
    assert_eq!(body["constructs"][0]["timeframe"], "1h");
    assert!(
        body["constructs"][0]["indicators"]
            .as_array()
            .unwrap()
            .len()
            >= 15
    );
}

#[tokio::test]
async fn alternative_me_formats_fear_and_greed_history() {
    let transport = MockTransport::with_responses([json!({"data":[
        {"value":"42","value_classification":"Fear"},
        {"value":"55","value_classification":"Neutral"}
    ]})]);
    let client = AlternativeMeClient::new(transport, "https://alternative.test", now(), Bangkok);
    let result = client
        .sentiment(&request("BTC-USD", 2026, 10, 6))
        .await
        .unwrap();
    let DataStatus::Available(snapshot) = result else {
        panic!("expected sentiment")
    };
    assert!(snapshot.summary.contains("42 (Fear)"));
    assert!(snapshot.summary.contains("55 (Neutral)"));
}

#[tokio::test]
async fn coinstats_fetches_btc_dominance_with_api_key() {
    let transport = MockTransport::with_responses([
        json!({"data":[[1, "58.2"]]}),
        json!({"data":[[1, "57.9"]]}),
    ]);
    let client = CoinStatsClient::new(
        transport.clone(),
        "https://coinstats.test",
        "cs-key",
        now(),
        Bangkok,
    );
    let result = client
        .btc_dominance(&request("BTC-USD", 2026, 10, 6))
        .await
        .unwrap();
    let DataStatus::Available(snapshot) = result else {
        panic!("expected dominance")
    };
    assert!(snapshot.summary.contains("24h: 58.2%"));
    assert!(snapshot.summary.contains("1w: 57.9%"));
    let requests = transport.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|r| header(r, "X-API-KEY") == Some("cs-key"))
    );
    assert_eq!(query(&requests[0], "type"), Some("24h"));
    assert_eq!(query(&requests[1], "type"), Some("1w"));
}

#[tokio::test]
async fn crypto_news_clients_parse_provider_shapes() {
    let coindesk_transport = MockTransport::with_responses([json!({"Data":[{
        "TITLE":"ETF flows rise","BODY":"Demand accelerated","SENTIMENT":"POSITIVE","URL":"https://example/cd"
    }]})]);
    let coindesk = CoinDeskClient::new(
        coindesk_transport.clone(),
        "https://coindesk.test",
        "cd-key",
        now(),
        Bangkok,
    );
    let DataStatus::Available(cd_news) = coindesk
        .news(&request("BTC-USD", 2026, 10, 6), 10)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(cd_news.items[0].contains("ETF flows rise"));
    assert_eq!(
        query(&coindesk_transport.requests()[0], "categories"),
        Some("BTC")
    );
    assert_eq!(
        query(&coindesk_transport.requests()[0], "api_key"),
        Some("cd-key")
    );

    let cs_transport = MockTransport::with_responses([json!({"result":[{
        "title":"Protocol update","source":"CoinStats","description":"Upgrade shipped"
    }]})]);
    let coinstats = CoinStatsClient::new(
        cs_transport,
        "https://coinstats.test",
        "cs-key",
        now(),
        Bangkok,
    );
    let DataStatus::Available(cs_news) = coinstats
        .news(&request("ETH-USD", 2026, 10, 6), 12)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(cs_news.items[0].contains("Protocol update"));

    let bb_transport = MockTransport::with_responses([json!({"data":{"data":[{
        "title":"快讯","create_time":"now","content":"市场更新"
    }]}})]);
    let blockbeats = BlockBeatsClient::new(bb_transport, "https://blockbeats.test", now(), Bangkok);
    let DataStatus::Available(bb_news) = blockbeats
        .news(&request("BTC-USD", 2026, 10, 6), 10)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(bb_news.items[0].contains("快讯"));
}
