use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelTierConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    #[serde(skip)]
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunConfig {
    pub quick: ModelTierConfig,
    pub deep: ModelTierConfig,
    pub output_language: String,
    pub analysts: Vec<String>,
    pub max_debate_rounds: u32,
    pub max_risk_rounds: u32,
}

impl RunConfig {
    pub fn safe_metadata(&self) -> Value {
        json!({
            "quick": {
                "provider": self.quick.provider,
                "model": self.quick.model,
                "base_url": self.quick.base_url,
            },
            "deep": {
                "provider": self.deep.provider,
                "model": self.deep.model,
                "base_url": self.deep.base_url,
            },
            "output_language": self.output_language,
            "analysts": self.analysts,
            "max_debate_rounds": self.max_debate_rounds,
            "max_risk_rounds": self.max_risk_rounds,
        })
    }

    pub fn fingerprint(&self) -> String {
        let bytes = serde_json::to_vec(&self.safe_metadata())
            .expect("safe run metadata is always JSON serializable");
        let digest = Sha256::digest(bytes);
        format!("{digest:x}")
    }
}
