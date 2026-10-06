use std::{fs, path::PathBuf};

use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use trading_agent_core::{ModelTierConfig, RunConfig, Symbol};
use trading_agent_llm::ProviderConfig;

use crate::{CliArgs, CliError};

pub trait EnvSource: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|value| !value.is_empty())
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedCliConfig {
    pub symbol: Symbol,
    pub trade_date: NaiveDate,
    pub run: RunConfig,
    pub results_dir: PathBuf,
    pub checkpoint_path: Option<PathBuf>,
    pub memory_path: PathBuf,
}

impl ResolvedCliConfig {
    pub fn safe_summary(&self) -> String {
        let quick = ProviderConfig {
            provider: self.run.quick.provider.clone(),
            model: self.run.quick.model.clone(),
            base_url: self.run.quick.base_url.clone(),
            api_key: self.run.quick.api_key.clone(),
            temperature: None,
            max_tokens: None,
            max_retries: 2,
        };
        let deep = ProviderConfig {
            provider: self.run.deep.provider.clone(),
            model: self.run.deep.model.clone(),
            base_url: self.run.deep.base_url.clone(),
            api_key: self.run.deep.api_key.clone(),
            temperature: None,
            max_tokens: None,
            max_retries: 2,
        };
        format!(
            "symbol={} date={} language={} analysts={:?} quick={} deep={} results_dir={}",
            self.symbol,
            self.trade_date,
            self.run.output_language,
            self.run.analysts,
            human_safe_display(&quick),
            human_safe_display(&deep),
            self.results_dir.display(),
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
struct FileConfig {
    quick_provider: Option<String>,
    quick_model: Option<String>,
    quick_base_url: Option<String>,
    deep_provider: Option<String>,
    deep_model: Option<String>,
    deep_base_url: Option<String>,
    output_language: Option<String>,
    analysts: Option<Vec<String>>,
    max_debate_rounds: Option<u32>,
    max_risk_rounds: Option<u32>,
    results_dir: Option<PathBuf>,
    checkpoint_path: Option<PathBuf>,
    memory_path: Option<PathBuf>,
}

pub fn resolve_config(args: &CliArgs, env: &dyn EnvSource) -> Result<ResolvedCliConfig, CliError> {
    let symbol = Symbol::parse(&args.symbol).map_err(|error| CliError::Config(error.to_string()))?;
    let trade_date = NaiveDate::parse_from_str(&args.date, "%Y-%m-%d")
        .map_err(|_| CliError::Config(format!("date must be YYYY-MM-DD, got {:?}", args.date)))?;
    if trade_date.format("%Y-%m-%d").to_string() != args.date {
        return Err(CliError::Config(format!("date must be canonical YYYY-MM-DD, got {:?}", args.date)));
    }
    let today = Utc::now().date_naive();
    if trade_date > today {
        return Err(CliError::Config(format!("date cannot be in the future: {trade_date}")));
    }

    let file = match &args.config {
        Some(path) => {
            let text = fs::read_to_string(path)
                .map_err(|error| CliError::Io(format!("read config {}: {error}", path.display())))?;
            serde_json::from_str::<FileConfig>(&text)
                .map_err(|error| CliError::Config(format!("parse config {}: {error}", path.display())))?
        }
        None => FileConfig::default(),
    };

    let quick_provider = choose(
        args.quick_provider.clone(),
        env.get("TRADINGAGENTS_QUICK_THINK_PROVIDER"),
        file.quick_provider,
        "openai".into(),
    );
    let quick_model = choose(
        args.quick_model.clone(),
        env.get("TRADINGAGENTS_QUICK_THINK_LLM"),
        file.quick_model,
        "gpt-6-luna".into(),
    );
    let quick_base_url = choose_opt(
        args.quick_base_url.clone(),
        env.get("TRADINGAGENTS_QUICK_THINK_BACKEND_URL"),
        file.quick_base_url,
    );

    let deep_provider = choose(
        args.deep_provider.clone(),
        env.get("TRADINGAGENTS_DEEP_THINK_PROVIDER"),
        file.deep_provider,
        "openai".into(),
    );
    let deep_model = choose(
        args.deep_model.clone(),
        env.get("TRADINGAGENTS_DEEP_THINK_LLM"),
        file.deep_model,
        "gpt-6-sol".into(),
    );
    let deep_base_url = choose_opt(
        args.deep_base_url.clone(),
        env.get("TRADINGAGENTS_DEEP_THINK_BACKEND_URL"),
        file.deep_base_url,
    );

    let output_language = choose(
        args.output_language.clone(),
        env.get("TRADINGAGENTS_OUTPUT_LANGUAGE"),
        file.output_language,
        "English".into(),
    );

    let analysts = if let Some(value) = args.analysts.clone() {
        value
    } else if let Some(raw) = env.get("TRADINGAGENTS_ANALYSTS") {
        split_list(&raw)
    } else if let Some(value) = file.analysts {
        value
    } else {
        vec!["market".into(), "sentiment".into(), "news".into(), "fundamentals".into()]
    };
    validate_analysts(&analysts)?;

    let max_debate_rounds = choose_u32(
        args.max_debate_rounds,
        env.get("TRADINGAGENTS_MAX_DEBATE_ROUNDS"),
        file.max_debate_rounds,
        1,
        "TRADINGAGENTS_MAX_DEBATE_ROUNDS",
    )?;
    let max_risk_rounds = choose_u32(
        args.max_risk_rounds,
        env.get("TRADINGAGENTS_MAX_RISK_ROUNDS"),
        file.max_risk_rounds,
        1,
        "TRADINGAGENTS_MAX_RISK_ROUNDS",
    )?;

    let results_dir = args.results_dir.clone()
        .or_else(|| env.get("TRADINGAGENTS_RESULTS_DIR").map(PathBuf::from))
        .or(file.results_dir)
        .unwrap_or_else(|| PathBuf::from("results"));
    let checkpoint_path = args.checkpoint.clone()
        .or_else(|| env.get("TRADINGAGENTS_CHECKPOINT_PATH").map(PathBuf::from))
        .or(file.checkpoint_path);
    let memory_path = args.memory.clone()
        .or_else(|| env.get("TRADINGAGENTS_MEMORY_LOG_PATH").map(PathBuf::from))
        .or(file.memory_path)
        .unwrap_or_else(|| results_dir.join("memory.json"));

    let quick = ModelTierConfig {
        provider: quick_provider.clone(),
        model: quick_model,
        base_url: quick_base_url,
        api_key: provider_api_key(&quick_provider, env),
    };
    let deep = ModelTierConfig {
        provider: deep_provider.clone(),
        model: deep_model,
        base_url: deep_base_url,
        api_key: provider_api_key(&deep_provider, env),
    };

    Ok(ResolvedCliConfig {
        symbol,
        trade_date,
        run: RunConfig {
            quick,
            deep,
            output_language,
            analysts,
            max_debate_rounds,
            max_risk_rounds,
        },
        results_dir,
        checkpoint_path,
        memory_path,
    })
}

fn choose(cli: Option<String>, env: Option<String>, file: Option<String>, default: String) -> String {
    cli.or(env).or(file).unwrap_or(default)
}

fn choose_opt(cli: Option<String>, env: Option<String>, file: Option<String>) -> Option<String> {
    cli.or(env).or(file)
}

fn choose_u32(
    cli: Option<u32>,
    env: Option<String>,
    file: Option<u32>,
    default: u32,
    env_name: &str,
) -> Result<u32, CliError> {
    if let Some(value) = cli {
        return Ok(value);
    }
    if let Some(raw) = env {
        return raw.parse::<u32>()
            .map_err(|_| CliError::Config(format!("{env_name} must be a non-negative integer")));
    }
    Ok(file.unwrap_or(default))
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(',').map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned).collect()
}

fn validate_analysts(analysts: &[String]) -> Result<(), CliError> {
    if analysts.is_empty() {
        return Err(CliError::Config("at least one analyst is required".into()));
    }
    for analyst in analysts {
        if !matches!(analyst.trim().to_ascii_lowercase().as_str(), "market" | "sentiment" | "news" | "fundamentals") {
            return Err(CliError::Config(format!("unsupported analyst: {analyst}")));
        }
    }
    Ok(())
}

fn provider_api_key(provider: &str, env: &dyn EnvSource) -> Option<String> {
    let keys: &[&str] = match provider.trim().to_ascii_lowercase().as_str() {
        "openai" | "openai_compatible" => &["OPENAI_API_KEY"],
        "qwen" | "dashscope" => &["DASHSCOPE_API_KEY"],
        "anthropic" => &["ANTHROPIC_API_KEY"],
        "google" | "gemini" => &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
        "azure" | "azure_openai" => &["AZURE_OPENAI_API_KEY"],
        "openrouter" => &["OPENROUTER_API_KEY"],
        "gitee" => &["GITEE_API_KEY"],
        "bedrock" | "aws_bedrock" => &["AWS_BEARER_TOKEN_BEDROCK"],
        _ => &[],
    };
    keys.iter().find_map(|key| env.get(key))
}

fn human_safe_display(config: &ProviderConfig) -> String {
    config
        .safe_display()
        .replace("%3Credacted%3E", "<redacted>")
        .replace("%3credacted%3e", "<redacted>")
}
