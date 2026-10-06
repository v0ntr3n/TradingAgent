"""Live crypto enrichment vendors ported from CryptoTradingAgents.

These helpers deliberately stay outside the point-in-time vendor router: Binance
futures positioning, TAAPI, Alternative.me, CoinDesk, CoinStats and BlockBeats
are live/recent feeds without a reliable historical vintage. The agent tools
that call them therefore withhold them for historical runs.
"""

from __future__ import annotations

import os
from datetime import UTC, datetime
from typing import Any

import requests

from tradingagents.dataflows.symbols import crypto_base

_BINANCE_FUTURES = "https://fapi.binance.com"
_TAAPI = "https://api.taapi.io"
_TIMEOUT = 15

_TAAPI_INDICATORS = (
    "ema",
    "ichimoku",
    "supertrend",
    "donchianchannels",
    "macd",
    "rsi",
    "stochrsi",
    "trix",
    "stc",
    "vwap",
    "atr",
    "bbands",
    "keltnerchannels",
    "chop",
    "engulfing",
    "hammer",
    "morningstar",
    "eveningstar",
    "3whitesoldiers",
    "3blackcrows",
)


def _crypto_base_or_none(symbol: str) -> str | None:
    return crypto_base(symbol)


def _binance_symbol(symbol: str) -> str | None:
    base = _crypto_base_or_none(symbol)
    return f"{base}USDT" if base else None


def _taapi_symbol(symbol: str) -> str | None:
    base = _crypto_base_or_none(symbol)
    return f"{base}/USDT" if base else None


def _get_json(url: str, *, params: dict[str, Any] | None = None, headers=None):
    response = requests.get(url, params=params, headers=headers, timeout=_TIMEOUT)
    response.raise_for_status()
    return response.json()


def _post_json(url: str, *, payload: dict[str, Any], headers=None):
    response = requests.post(url, json=payload, headers=headers, timeout=_TIMEOUT)
    response.raise_for_status()
    return response.json()


def _timestamp(ms: Any) -> str:
    try:
        return datetime.fromtimestamp(float(ms) / 1000, tz=UTC).strftime("%Y-%m-%d %H:%M UTC")
    except (TypeError, ValueError, OSError):
        return str(ms)


def fetch_binance_market_summary(
    symbol: str,
    interval: str = "15m",
    *,
    klines_limit: int = 24,
    depth_limit: int = 20,
    ratio_limit: int = 12,
) -> str:
    """Current USD-M futures market microstructure for a crypto pair."""
    pair = _binance_symbol(symbol)
    if not pair:
        return f"<Binance crypto enrichment unavailable: {symbol!r} is not a supported crypto pair>"

    try:
        klines = _get_json(
            f"{_BINANCE_FUTURES}/fapi/v1/klines",
            params={"symbol": pair, "interval": interval, "limit": max(1, min(150, klines_limit))},
        )
        depth = _get_json(
            f"{_BINANCE_FUTURES}/fapi/v1/depth",
            params={"symbol": pair, "limit": max(5, min(100, depth_limit))},
        )
        ticker = _get_json(
            f"{_BINANCE_FUTURES}/fapi/v1/ticker/24hr",
            params={"symbol": pair},
        )

        ratio_params = {
            "symbol": pair,
            "period": interval if interval in {"5m", "15m", "30m", "1h", "2h", "4h", "6h", "12h", "1d"} else "15m",
            "limit": max(1, min(30, ratio_limit)),
        }
        top_positions = _get_json(
            f"{_BINANCE_FUTURES}/futures/data/topLongShortPositionRatio",
            params=ratio_params,
        )
        top_accounts = _get_json(
            f"{_BINANCE_FUTURES}/futures/data/topLongShortAccountRatio",
            params=ratio_params,
        )
        global_accounts = _get_json(
            f"{_BINANCE_FUTURES}/futures/data/globalLongShortAccountRatio",
            params=ratio_params,
        )
        taker = _get_json(
            f"{_BINANCE_FUTURES}/futures/data/takerlongshortRatio",
            params=ratio_params,
        )
    except (requests.RequestException, ValueError) as exc:
        return f"<Binance crypto enrichment unavailable: {exc}>"

    lines = [
        f"# Binance USD-M futures snapshot — {pair}",
        f"Interval: {ratio_params['period']}",
        "",
        "## 24-hour ticker",
        (
            f"Last: {ticker.get('lastPrice', 'N/A')} | Change: "
            f"{ticker.get('priceChangePercent', 'N/A')}% | "
            f"High: {ticker.get('highPrice', 'N/A')} | Low: {ticker.get('lowPrice', 'N/A')} | "
            f"Quote volume: {ticker.get('quoteVolume', 'N/A')}"
        ),
    ]

    bids = depth.get("bids") or []
    asks = depth.get("asks") or []
    if bids and asks:
        lines += [
            "",
            "## Order book top",
            f"Best bid: {bids[0][0]} ({bids[0][1]}) | Best ask: {asks[0][0]} ({asks[0][1]})",
        ]

    if klines:
        lines += ["", f"## Recent {ratio_params['period']} candles (oldest → newest)"]
        for row in klines[-12:]:
            lines.append(
                f"{_timestamp(row[0])}: O {row[1]} H {row[2]} L {row[3]} C {row[4]} V {row[5]}"
            )

    def add_ratio(title: str, rows: Any, key: str = "longShortRatio"):
        if isinstance(rows, list) and rows:
            lines.extend(["", f"## {title}"])
            for row in rows[-8:]:
                value = row.get(key, "N/A")
                lines.append(f"{_timestamp(row.get('timestamp'))}: {value}")

    add_ratio("Top trader position long/short ratio", top_positions)
    add_ratio("Top trader account long/short ratio", top_accounts)
    add_ratio("Global account long/short ratio", global_accounts)
    add_ratio("Taker buy/sell ratio", taker, "buySellRatio")

    return "\n".join(lines)


def fetch_taapi_bulk(symbol: str, interval: str = "15m") -> str:
    """Bulk current crypto indicators from TAAPI.io using its current API shape."""
    api_key = os.getenv("TAAPI_API_KEY")
    if not api_key:
        return "<TAAPI unavailable: TAAPI_API_KEY is not configured>"

    pair = _taapi_symbol(symbol)
    if not pair:
        return f"<TAAPI unavailable: {symbol!r} is not a supported crypto pair>"

    payload = {
        "constructs": [
            {
                "id": "crypto",
                "exchange": "binance",
                "symbol": pair,
                "timeframe": interval,
                "indicators": [
                    {"id": indicator, "indicator": indicator}
                    for indicator in _TAAPI_INDICATORS
                ],
            }
        ]
    }
    headers = {"Authorization": f"Bearer {api_key}"}

    try:
        data = _post_json(f"{_TAAPI}/bulk", payload=payload, headers=headers)
    except (requests.RequestException, ValueError) as exc:
        return f"<TAAPI unavailable: {exc}>"

    return f"# TAAPI bulk indicators — {pair} ({interval})\n{data}"


def fetch_fear_and_greed() -> str:
    """Current Alternative.me crypto Fear & Greed readings."""
    try:
        data = _get_json("https://api.alternative.me/fng/", params={"limit": 10})
    except (requests.RequestException, ValueError) as exc:
        return f"<Fear & Greed unavailable: {exc}>"

    rows = data.get("data") if isinstance(data, dict) else None
    if not rows:
        return "<Fear & Greed unavailable: no data returned>"

    formatted = []
    for row in rows:
        formatted.append(
            f"{row.get('value', 'N/A')} ({row.get('value_classification', 'N/A')})"
        )
    return "# Crypto Fear & Greed Index\nCurrent and previous daily readings: " + ", ".join(formatted)


def fetch_coinstats_btc_dominance() -> str:
    """Current BTC dominance from CoinStats when configured."""
    api_key = os.getenv("COINSTATS_API_KEY")
    if not api_key:
        return "<CoinStats BTC dominance unavailable: COINSTATS_API_KEY is not configured>"

    headers = {"accept": "application/json", "X-API-KEY": api_key}
    values: dict[str, Any] = {}
    try:
        for label, period in (("24h", "24h"), ("1w", "1w")):
            data = _get_json(
                "https://openapiv1.coinstats.app/insights/btc-dominance",
                params={"type": period},
                headers=headers,
            )
            rows = data.get("data") if isinstance(data, dict) else None
            if rows:
                values[label] = rows[-1][1]
    except (requests.RequestException, ValueError, IndexError, TypeError) as exc:
        return f"<CoinStats BTC dominance unavailable: {exc}>"

    if not values:
        return "<CoinStats BTC dominance unavailable: no data returned>"
    return "# Bitcoin dominance\n" + " | ".join(f"{k}: {v}%" for k, v in values.items())


def fetch_coindesk_news(symbol: str, limit: int = 10) -> str:
    """Latest CoinDesk Data API articles for a crypto base when configured."""
    api_key = os.getenv("COINDESK_API_KEY")
    if not api_key:
        return "<CoinDesk unavailable: COINDESK_API_KEY is not configured>"

    base = _crypto_base_or_none(symbol)
    params: dict[str, Any] = {
        "lang": "EN",
        "limit": max(1, min(50, limit)),
        "api_key": api_key,
    }
    if base:
        params["categories"] = base
    try:
        data = _get_json("https://data-api.coindesk.com/news/v1/article/list", params=params)
    except (requests.RequestException, ValueError) as exc:
        return f"<CoinDesk unavailable: {exc}>"

    rows = data.get("Data") if isinstance(data, dict) else None
    if not rows:
        return "<CoinDesk unavailable: no articles returned>"

    lines = ["# CoinDesk crypto news"]
    for row in rows[:limit]:
        title = row.get("TITLE", "")
        body = row.get("BODY", "")
        sentiment = row.get("SENTIMENT", "")
        url = row.get("URL", "")
        lines.append(f"- **{title}** [{sentiment}] — {body} {url}".strip())
    return "\n".join(lines)


def fetch_coinstats_news(limit: int = 12) -> str:
    """Latest CoinStats crypto news when configured."""
    api_key = os.getenv("COINSTATS_API_KEY")
    if not api_key:
        return "<CoinStats news unavailable: COINSTATS_API_KEY is not configured>"

    headers = {"accept": "application/json", "X-API-KEY": api_key}
    try:
        data = _get_json(
            "https://openapiv1.coinstats.app/news/type/latest",
            params={"page": 1, "limit": max(1, min(20, limit))},
            headers=headers,
        )
    except (requests.RequestException, ValueError) as exc:
        return f"<CoinStats news unavailable: {exc}>"

    rows = data if isinstance(data, list) else data.get("result") if isinstance(data, dict) else None
    if not rows:
        return "<CoinStats news unavailable: no articles returned>"

    lines = ["# CoinStats crypto news"]
    for row in rows[:limit]:
        lines.append(
            f"- **{row.get('title', '')}** ({row.get('source', 'CoinStats')}) — "
            f"{row.get('description', '')}"
        )
    return "\n".join(lines)


def fetch_blockbeats_news(limit: int = 10) -> str:
    """Latest BlockBeats flash news (Chinese)."""
    try:
        data = _get_json(
            "https://api.theblockbeats.news/v1/open-api/open-flash",
            params={"page": 1, "size": max(1, min(30, limit)), "type": "push", "lang": "cn"},
        )
    except (requests.RequestException, ValueError) as exc:
        return f"<BlockBeats unavailable: {exc}>"

    rows = (
        data.get("data", {}).get("data")
        if isinstance(data, dict) and isinstance(data.get("data"), dict)
        else None
    )
    if not rows:
        return "<BlockBeats unavailable: no articles returned>"

    lines = ["# BlockBeats flash news"]
    for row in rows[:limit]:
        lines.append(
            f"- **{row.get('title', '')}** ({row.get('create_time', '')}) — "
            f"{row.get('content', '')}"
        )
    return "\n".join(lines)
