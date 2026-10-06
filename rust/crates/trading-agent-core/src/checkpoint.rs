use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{AgentState, CoreError, SCHEMA_VERSION, agents::Evidence, workflow::RunInput};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSignature(pub String);

impl RunSignature {
    pub fn from_inputs(input: &RunInput) -> Self {
        let bytes = serde_json::to_vec(&behavioral_inputs(input))
            .expect("run signature inputs are JSON serializable");
        Self(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn safe_metadata(input: &RunInput) -> Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "symbol": input.state.symbol.to_string(),
            "asset_type": input.state.asset_type,
            "trade_date": input.state.trade_date,
            "instrument_context": input.state.instrument_context,
            "investment_preferences": input.state.investment_preferences,
            "external_reports": input.state.external_reports,
            "portfolio": input.state.portfolio,
            "past_context": input.state.past_context,
            "evidence": safe_evidence(&input.evidence),
            "config": {
                "quick": safe_tier(
                    &input.config.quick.provider,
                    &input.config.quick.model,
                    input.config.quick.base_url.as_deref(),
                    input.config.quick.api_key.is_some(),
                ),
                "deep": safe_tier(
                    &input.config.deep.provider,
                    &input.config.deep.model,
                    input.config.deep.base_url.as_deref(),
                    input.config.deep.api_key.is_some(),
                ),
                "output_language": input.config.output_language,
                "analysts": input.config.analysts,
                "max_debate_rounds": input.config.max_debate_rounds,
                "max_risk_rounds": input.config.max_risk_rounds,
            }
        })
    }
}

fn behavioral_inputs(input: &RunInput) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "state": {
            "symbol": input.state.symbol.to_string(),
            "asset_type": input.state.asset_type,
            "trade_date": input.state.trade_date,
            "instrument_context": input.state.instrument_context,
            "investment_preferences": input.state.investment_preferences,
            "external_reports": input.state.external_reports,
            "portfolio": input.state.portfolio,
            "past_context": input.state.past_context,
        },
        "evidence": safe_evidence(&input.evidence),
        "config": {
            "quick": {
                "provider": input.config.quick.provider,
                "model": input.config.quick.model,
                "base_url": input.config.quick.base_url,
                "api_key": input.config.quick.api_key,
            },
            "deep": {
                "provider": input.config.deep.provider,
                "model": input.config.deep.model,
                "base_url": input.config.deep.base_url,
                "api_key": input.config.deep.api_key,
            },
            "output_language": input.config.output_language,
            "analysts": input.config.analysts,
            "max_debate_rounds": input.config.max_debate_rounds,
            "max_risk_rounds": input.config.max_risk_rounds,
        }
    })
}

fn safe_evidence(evidence: &crate::agents::AgentEvidence) -> Value {
    json!({
        "market": evidence_value(&evidence.market),
        "sentiment": evidence_value(&evidence.sentiment),
        "news": evidence_value(&evidence.news),
        "fundamentals": evidence_value(&evidence.fundamentals),
    })
}

fn evidence_value(evidence: &Evidence) -> Value {
    match evidence {
        Evidence::Available(content) => json!({"status": "available", "content": content}),
        Evidence::Unavailable { source, reason } => {
            json!({"status": "unavailable", "source": source, "reason": reason})
        }
        Evidence::WithheldHistorical { source, as_of } => {
            json!({"status": "withheld_historical", "source": source, "as_of": as_of})
        }
    }
}

fn safe_tier(provider: &str, model: &str, base_url: Option<&str>, has_api_key: bool) -> Value {
    json!({
        "provider": provider,
        "model": model,
        "base_url": base_url.map(redact_url),
        "api_key": has_api_key.then_some("<redacted>"),
    })
}

fn redact_url(raw: &str) -> String {
    let Ok(mut url) = Url::parse(raw) else {
        return "<redacted-invalid-url>".into();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    let pairs = url
        .query_pairs()
        .map(|(key, value)| {
            let key = key.into_owned();
            let lowered = key.to_ascii_lowercase();
            let sensitive = lowered.contains("key")
                || lowered.contains("token")
                || lowered.contains("secret")
                || lowered.contains("password")
                || lowered == "sig"
                || lowered == "signature";
            (
                key,
                if sensitive {
                    "redacted".to_string()
                } else {
                    value.into_owned()
                },
            )
        })
        .collect::<Vec<_>>();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    url.to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CompletedStage {
    Analysts,
    Research,
    Trader,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointEnvelope {
    pub schema_version: u32,
    pub signature: RunSignature,
    pub completed_stage: CompletedStage,
    pub state: AgentState,
}

pub trait CheckpointStore: Send + Sync {
    fn load(&self) -> Result<Option<CheckpointEnvelope>, CoreError>;
    fn save(&self, envelope: &CheckpointEnvelope) -> Result<(), CoreError>;
    fn clear(&self) -> Result<(), CoreError>;
}

#[derive(Clone, Debug)]
pub struct JsonCheckpointStore {
    path: PathBuf,
}

impl JsonCheckpointStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl CheckpointStore for JsonCheckpointStore {
    fn load(&self) -> Result<Option<CheckpointEnvelope>, CoreError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&self.path)
            .map_err(|error| CoreError::Persistence(format!("read checkpoint: {error}")))?;
        let envelope = serde_json::from_slice(&bytes)?;
        Ok(Some(envelope))
    }

    fn save(&self, envelope: &CheckpointEnvelope) -> Result<(), CoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CoreError::Persistence(format!("create checkpoint directory: {error}"))
            })?;
        }
        let bytes = serde_json::to_vec_pretty(envelope)?;
        let temp_path = self.path.with_extension("tmp");
        fs::write(&temp_path, bytes)
            .map_err(|error| CoreError::Persistence(format!("write checkpoint: {error}")))?;
        fs::rename(&temp_path, &self.path)
            .map_err(|error| CoreError::Persistence(format!("replace checkpoint: {error}")))?;
        Ok(())
    }

    fn clear(&self) -> Result<(), CoreError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(CoreError::Persistence(format!("clear checkpoint: {error}"))),
        }
    }
}
