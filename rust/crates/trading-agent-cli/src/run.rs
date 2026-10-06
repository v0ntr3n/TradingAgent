use std::{fs, path::Path, str::FromStr, sync::Arc};

use chrono::Utc;
use chrono_tz::UTC;
use rust_decimal::Decimal;
use serde::Deserialize;
use trading_agent_core::{
    AgentState, ExternalReport, InvestmentPreferences, PortfolioContext, Position,
    agents::{AgentEvidence, Evidence},
    checkpoint::JsonCheckpointStore,
    memory::JsonDecisionMemory,
    workflow::{RunInput, RunResult, WorkflowRunner},
};
use trading_agent_data::{
    DataRequest, DataStatus, MarketDataSource, ReqwestTransport, SentimentSource,
    crypto::{AlternativeMeClient, BinanceClient, BlockBeatsClient, CoinDeskClient, CoinStatsClient, TaapiClient},
    general::{PolymarketProvider, YahooProvider},
};
use trading_agent_llm::{
    ProviderConfig, ProviderRegistry, ReqwestLlmTransport, ResearchClient, ResearchRequest,
    SearchResearchClient,
};
use trading_agent_reporting::ReportWriter;

use crate::{CliArgs, CliError, EnvSource, ResolvedCliConfig, resolve_config};

#[derive(Deserialize)]
struct PortfolioFile {
    cash: Option<Decimal>,
    currency: Option<String>,
    #[serde(default)]
    positions: Vec<PositionFile>,
}

#[derive(Deserialize)]
struct PositionFile {
    symbol: String,
    quantity: Decimal,
    average_price: Option<Decimal>,
}

pub fn build_run_input(args: &CliArgs, resolved: &ResolvedCliConfig) -> Result<RunInput, CliError> {
    let mut state = AgentState::new(resolved.symbol.clone(), resolved.trade_date);

    if let Some(path) = &args.preferences {
        let text = read_text(path, "investment preferences")?;
        state.investment_preferences = Some(
            InvestmentPreferences::from_json_str(&text)
                .map_err(|error| CliError::Config(error.to_string()))?,
        );
    }

    for path in &args.external_reports {
        let content = read_text(path, "external report")?;
        state.external_reports.push(ExternalReport {
            title: path.file_name().and_then(|value| value.to_str()).map(str::to_owned),
            source: Some(path.display().to_string()),
            content,
        });
    }

    if let Some(path) = &args.portfolio {
        let text = read_text(path, "portfolio")?;
        let parsed: PortfolioFile = serde_json::from_str(&text)
            .map_err(|error| CliError::Config(format!("parse portfolio {}: {error}", path.display())))?;
        let mut positions = Vec::with_capacity(parsed.positions.len());
        for position in parsed.positions {
            positions.push(Position {
                symbol: trading_agent_core::Symbol::parse(&position.symbol)
                    .map_err(|error| CliError::Config(error.to_string()))?,
                quantity: position.quantity,
                average_price: position.average_price,
            });
        }
        state.portfolio = Some(PortfolioContext {
            cash: parsed.cash,
            currency: parsed.currency,
            positions,
        });
    }

    Ok(RunInput {
        state,
        evidence: AgentEvidence {
            market: Evidence::Unavailable { source: "cli".into(), reason: "market evidence not collected yet".into() },
            sentiment: Evidence::Unavailable { source: "cli".into(), reason: "sentiment evidence not collected yet".into() },
            news: Evidence::Unavailable { source: "cli".into(), reason: "news evidence not collected yet".into() },
            fundamentals: Evidence::Unavailable { source: "cli".into(), reason: "fundamentals evidence not collected yet".into() },
        },
        config: resolved.run.clone(),
    })
}

pub async fn run_from_args(args: CliArgs, env: &dyn EnvSource) -> Result<RunResult, CliError> {
    let resolved = resolve_config(&args, env)?;
    let mut input = build_run_input(&args, &resolved)?;

    let llm_transport = Arc::new(ReqwestLlmTransport::new());
    let registry = ProviderRegistry::new(llm_transport);
    let quick = registry.create(&provider_config(&resolved.run.quick))
        .map_err(|error| CliError::Runtime(error.to_string()))?;
    let deep = registry.create(&provider_config(&resolved.run.deep))
        .map_err(|error| CliError::Runtime(error.to_string()))?;

    input.evidence = collect_evidence(&resolved, env, quick.clone()).await;

    let report_root = resolved
        .results_dir
        .join(input.state.symbol.as_str())
        .join(resolved.trade_date.format("%Y-%m-%d").to_string());
    let writer = Arc::new(ReportWriter::new(&report_root)
        .map_err(|error| CliError::Runtime(error.to_string()))?);

    let memory = Arc::new(JsonDecisionMemory::new(&resolved.memory_path));
    let mut runner = WorkflowRunner::new(quick, deep)
        .with_decision_memory(memory)
        .with_event_sink(writer.clone());

    if let Some(path) = &resolved.checkpoint_path {
        runner = runner.with_checkpoint_store(Arc::new(JsonCheckpointStore::new(path)));
    }

    let result = runner.run(input).await
        .map_err(|error| CliError::Runtime(error.to_string()))?;
    writer.write_complete(&result)
        .map_err(|error| CliError::Runtime(error.to_string()))?;
    Ok(result)
}

fn provider_config(tier: &trading_agent_core::ModelTierConfig) -> ProviderConfig {
    ProviderConfig {
        provider: tier.provider.clone(),
        model: tier.model.clone(),
        base_url: tier.base_url.clone(),
        api_key: tier.api_key.clone(),
        temperature: None,
        max_tokens: None,
        max_retries: 2,
    }
}

async fn collect_evidence(
    resolved: &ResolvedCliConfig,
    env: &dyn EnvSource,
    quick: Arc<dyn trading_agent_llm::LlmClient>,
) -> AgentEvidence {
    let request = DataRequest {
        symbol: resolved.symbol.clone(),
        as_of: resolved.trade_date,
    };
    let now = Utc::now();
    let today = now.date_naive();
    let http = Arc::new(ReqwestTransport::new());

    if resolved.symbol.asset_type() == trading_agent_core::AssetType::Crypto {
        let binance = BinanceClient::new(http.clone(), "https://fapi.binance.com", now, UTC);
        let market = match binance.market_snapshot(&request, "15m").await {
            Ok(status) => market_evidence(status),
            Err(error) => unavailable("binance", error.to_string()),
        };
        let market = if let Some(key) = env.get("TAAPI_API_KEY") {
            let taapi = TaapiClient::new(http.clone(), "https://api.taapi.io", key, now, UTC);
            match taapi.indicators(&request, "15m").await {
                Ok(status) => combine_evidence("crypto-market", vec![market, technical_evidence(status)]),
                Err(error) => combine_evidence("crypto-market", vec![market, unavailable("taapi", error.to_string())]),
            }
        } else {
            market
        };

        let sentiment_client = AlternativeMeClient::new(http.clone(), "https://api.alternative.me", now, UTC);
        let sentiment = match sentiment_client.sentiment(&request).await {
            Ok(status) => sentiment_evidence(status),
            Err(error) => unavailable("alternative_me", error.to_string()),
        };

        let blockbeats = BlockBeatsClient::new(http.clone(), "https://api.theblockbeats.news", now, UTC);
        let mut news_parts = vec![match blockbeats.news(&request, 10).await {
            Ok(status) => news_evidence(status),
            Err(error) => unavailable("blockbeats", error.to_string()),
        }];
        if let Some(key) = env.get("COINDESK_API_KEY") {
            let client = CoinDeskClient::new(http.clone(), "https://data-api.coindesk.com", key, now, UTC);
            news_parts.push(match client.news(&request, 10).await {
                Ok(status) => news_evidence(status),
                Err(error) => unavailable("coindesk", error.to_string()),
            });
        }
        if let Some(key) = env.get("COINSTATS_API_KEY") {
            let client = CoinStatsClient::new(http.clone(), "https://openapiv1.coinstats.app", key.clone(), now, UTC);
            news_parts.push(match client.news(&request, 10).await {
                Ok(status) => news_evidence(status),
                Err(error) => unavailable("coinstats", error.to_string()),
            });
        }
        let news = combine_evidence("crypto-news", news_parts);

        let mut fundamental_parts = Vec::new();
        if let Some(key) = env.get("COINSTATS_API_KEY") {
            let client = CoinStatsClient::new(http.clone(), "https://openapiv1.coinstats.app", key, now, UTC);
            fundamental_parts.push(match client.btc_dominance(&request).await {
                Ok(status) => fundamentals_evidence(status),
                Err(error) => unavailable("coinstats", error.to_string()),
            });
        }
        fundamental_parts.push(search_evidence(
            quick,
            today,
            resolved.trade_date,
            format!(
                "Research current fundamentals, ecosystem developments, token economics, regulatory and macro factors relevant to {}. Cite only information available now.",
                resolved.symbol
            ),
            "search-research",
        ).await);
        let fundamentals = combine_evidence("crypto-fundamentals", fundamental_parts);

        AgentEvidence { market, sentiment, news, fundamentals }
    } else {
        let market = if resolved.trade_date == today {
            let yahoo = YahooProvider::new(http.clone(), "https://query1.finance.yahoo.com");
            match yahoo.market_snapshot(&request).await {
                Ok(status) => market_evidence(status),
                Err(error) => unavailable("yahoo", error.to_string()),
            }
        } else {
            Evidence::WithheldHistorical { source: "stock-live-market".into(), as_of: resolved.trade_date }
        };

        let sentiment = if resolved.trade_date == today {
            let polymarket = PolymarketProvider::new(http, "https://gamma-api.polymarket.com", today);
            match polymarket.sentiment(&request).await {
                Ok(status) => sentiment_evidence(status),
                Err(error) => unavailable("polymarket", error.to_string()),
            }
        } else {
            Evidence::WithheldHistorical { source: "stock-live-sentiment".into(), as_of: resolved.trade_date }
        };

        let news = search_evidence(
            quick.clone(),
            today,
            resolved.trade_date,
            format!("Research material news about {} relevant to an investment decision.", resolved.symbol),
            "search-news",
        ).await;
        let fundamentals = search_evidence(
            quick,
            today,
            resolved.trade_date,
            format!("Research current financial and fundamental information about {} relevant to an investment decision.", resolved.symbol),
            "search-fundamentals",
        ).await;

        AgentEvidence { market, sentiment, news, fundamentals }
    }
}

async fn search_evidence(
    client: Arc<dyn trading_agent_llm::LlmClient>,
    today: chrono::NaiveDate,
    as_of: chrono::NaiveDate,
    query: String,
    source: &str,
) -> Evidence {
    let research = SearchResearchClient::new(client, today);
    match research.research(ResearchRequest { query, as_of }).await {
        Ok(response) => Evidence::Available(response.content),
        Err(trading_agent_llm::LlmError::HistoricalSearchRefused { .. }) => {
            Evidence::WithheldHistorical { source: source.into(), as_of }
        }
        Err(error) => unavailable(source, error.to_string()),
    }
}

fn unavailable(source: &str, reason: String) -> Evidence {
    Evidence::Unavailable { source: source.into(), reason }
}

fn combine_evidence(source: &str, parts: Vec<Evidence>) -> Evidence {
    let mut available = Vec::new();
    let mut withheld = None;
    let mut unavailable_parts = Vec::new();
    for part in parts {
        match part {
            Evidence::Available(content) => available.push(content),
            Evidence::WithheldHistorical { as_of, .. } => withheld = Some(as_of),
            Evidence::Unavailable { source, reason } => unavailable_parts.push(format!("{source}: {reason}")),
        }
    }
    if !available.is_empty() {
        if !unavailable_parts.is_empty() {
            available.push(format!("Unavailable supplemental sources: {}", unavailable_parts.join("; ")));
        }
        return Evidence::Available(available.join("\n\n"));
    }
    if let Some(as_of) = withheld {
        return Evidence::WithheldHistorical { source: source.into(), as_of };
    }
    unavailable(source, unavailable_parts.join("; "))
}

fn market_evidence(status: DataStatus<trading_agent_data::MarketSnapshot>) -> Evidence {
    match status {
        DataStatus::Available(value) => Evidence::Available(value.summary),
        DataStatus::Unavailable { source, reason } => Evidence::Unavailable { source, reason },
        DataStatus::WithheldHistorical { source, as_of } => Evidence::WithheldHistorical { source, as_of },
    }
}

fn technical_evidence(status: DataStatus<trading_agent_data::TechnicalIndicators>) -> Evidence {
    match status {
        DataStatus::Available(value) => Evidence::Available(value.summary),
        DataStatus::Unavailable { source, reason } => Evidence::Unavailable { source, reason },
        DataStatus::WithheldHistorical { source, as_of } => Evidence::WithheldHistorical { source, as_of },
    }
}

fn sentiment_evidence(status: DataStatus<trading_agent_data::SentimentSnapshot>) -> Evidence {
    match status {
        DataStatus::Available(value) => Evidence::Available(value.summary),
        DataStatus::Unavailable { source, reason } => Evidence::Unavailable { source, reason },
        DataStatus::WithheldHistorical { source, as_of } => Evidence::WithheldHistorical { source, as_of },
    }
}

fn news_evidence(status: DataStatus<trading_agent_data::NewsBatch>) -> Evidence {
    match status {
        DataStatus::Available(value) => Evidence::Available(value.items.join("\n")),
        DataStatus::Unavailable { source, reason } => Evidence::Unavailable { source, reason },
        DataStatus::WithheldHistorical { source, as_of } => Evidence::WithheldHistorical { source, as_of },
    }
}

fn fundamentals_evidence(status: DataStatus<trading_agent_data::FundamentalsSnapshot>) -> Evidence {
    match status {
        DataStatus::Available(value) => Evidence::Available(value.summary),
        DataStatus::Unavailable { source, reason } => Evidence::Unavailable { source, reason },
        DataStatus::WithheldHistorical { source, as_of } => Evidence::WithheldHistorical { source, as_of },
    }
}

fn read_text(path: &Path, label: &str) -> Result<String, CliError> {
    fs::read_to_string(path)
        .map_err(|error| CliError::Io(format!("read {label} {}: {error}", path.display())))
}
