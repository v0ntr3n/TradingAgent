use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::checkpoint::RunSignature;
use crate::{CoreError, workflow::FinalRating};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStage {
    Analysts,
    Research,
    Trader,
    Risk,
    Portfolio,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RunEvent {
    Started {
        signature: RunSignature,
        metadata: Value,
    },
    SectionCompleted {
        stage: WorkflowStage,
        section: String,
        content: String,
    },
    Completed {
        rating: FinalRating,
    },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: &RunEvent) -> Result<(), CoreError>;
}
