import pytest

from tradingagents.agents.tools import get_crypto_market_data, get_crypto_news
from tradingagents.dataflows.vendors import crypto


@pytest.mark.unit
def test_live_crypto_tools_are_withheld_for_historical_runs():
    market = get_crypto_market_data.func("BTC-USD", "15m", "2020-01-01")
    news = get_crypto_news.func("BTC-USD", 5, "2020-01-01")

    assert "withheld for historical run" in market
    assert "look-ahead bias" in market
    assert "withheld for historical run" in news


@pytest.mark.unit
def test_binance_summary_normalizes_yahoo_crypto_symbol(monkeypatch):
    def fake_get(url, *, params=None, headers=None):
        if url.endswith("/klines"):
            return [[1_700_000_000_000, "1", "2", "0.5", "1.5", "10"]]
        if url.endswith("/depth"):
            return {"bids": [["1.49", "5"]], "asks": [["1.51", "4"]]}
        if url.endswith("/ticker/24hr"):
            return {
                "lastPrice": "1.50",
                "priceChangePercent": "2.5",
                "highPrice": "1.60",
                "lowPrice": "1.30",
                "quoteVolume": "1000",
            }
        if url.endswith("/takerlongshortRatio"):
            return [{"timestamp": 1_700_000_000_000, "buySellRatio": "1.2"}]
        return [{"timestamp": 1_700_000_000_000, "longShortRatio": "1.1"}]

    monkeypatch.setattr(crypto, "_get_json", fake_get)

    report = crypto.fetch_binance_market_summary("BTC-USD", "15m")

    assert "BTCUSDT" in report
    assert "Last: 1.50" in report
    assert "Best bid: 1.49" in report
    assert "Taker buy/sell ratio" in report


@pytest.mark.unit
def test_taapi_uses_current_bulk_payload(monkeypatch):
    captured = {}

    def fake_post(url, *, payload, headers=None):
        captured.update(url=url, payload=payload, headers=headers)
        return {"data": [{"id": "rsi", "result": {"value": 50}}]}

    monkeypatch.setenv("TAAPI_API_KEY", "test-key")
    monkeypatch.setattr(crypto, "_post_json", fake_post)

    report = crypto.fetch_taapi_bulk("ETH-USDT", "1h")

    construct = captured["payload"]["constructs"][0]
    assert captured["url"].endswith("/bulk")
    assert captured["headers"]["Authorization"] == "Bearer test-key"
    assert construct["symbol"] == "ETH/USDT"
    assert construct["timeframe"] == "1h"
    assert "rsi" in report
