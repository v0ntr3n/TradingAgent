mod debate;
mod runner;

pub use runner::{RunInput, RunResult, WorkflowRunner};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinalRating {
    Buy,
    Overweight,
    Hold,
    Underweight,
    Sell,
    Review,
}
