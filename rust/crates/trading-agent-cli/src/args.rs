use std::path::PathBuf;

use clap::Parser;

#[derive(Clone, Debug, Parser)]
#[command(name = "trading-agent", about = "TradingAgent native Rust CLI")]
pub struct CliArgs {
    /// Ticker or crypto symbol, for example AAPL, BTC-USD, or BTCUSDT.
    pub symbol: String,

    /// Analysis date in YYYY-MM-DD format.
    #[arg(long)]
    pub date: String,

    /// Comma-separated analyst set.
    #[arg(long, value_delimiter = ',')]
    pub analysts: Option<Vec<String>>,

    #[arg(long)]
    pub output_language: Option<String>,

    #[arg(long)]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub quick_provider: Option<String>,
    #[arg(long)]
    pub quick_model: Option<String>,
    #[arg(long)]
    pub quick_base_url: Option<String>,

    #[arg(long)]
    pub deep_provider: Option<String>,
    #[arg(long)]
    pub deep_model: Option<String>,
    #[arg(long)]
    pub deep_base_url: Option<String>,

    #[arg(long)]
    pub max_debate_rounds: Option<u32>,
    #[arg(long)]
    pub max_risk_rounds: Option<u32>,

    #[arg(long)]
    pub preferences: Option<PathBuf>,

    #[arg(long = "external-report")]
    pub external_reports: Vec<PathBuf>,

    #[arg(long)]
    pub portfolio: Option<PathBuf>,

    #[arg(long)]
    pub results_dir: Option<PathBuf>,

    #[arg(long)]
    pub checkpoint: Option<PathBuf>,

    #[arg(long)]
    pub memory: Option<PathBuf>,
}
