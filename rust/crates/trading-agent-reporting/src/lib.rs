use std::{
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;
use trading_agent_core::{
    events::RunEvent,
    workflow::{FinalRating, RunResult},
};

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("report I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("report JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsafe report section name: {0}")]
    UnsafeSection(String),
}

#[derive(Clone, Debug)]
pub struct ReportWriter {
    root: PathBuf,
}

impl ReportWriter {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ReportError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("reports"))?;
        Ok(Self { root })
    }

    pub fn write_event(&self, event: &RunEvent) -> Result<(), ReportError> {
        match event {
            RunEvent::Started { metadata, .. } => {
                self.atomic_write(
                    &self.root.join("run_metadata.json"),
                    serde_json::to_string_pretty(metadata)?.as_bytes(),
                )?;
            }
            RunEvent::SectionCompleted {
                section, content, ..
            } => {
                validate_section(section)?;
                let mut body = content.clone();
                if !body.ends_with('\n') {
                    body.push('\n');
                }
                self.atomic_write(
                    &self.root.join("reports").join(format!("{section}.md")),
                    body.as_bytes(),
                )?;
            }
            RunEvent::Completed { .. } => {}
        }
        Ok(())
    }

    pub fn write_complete(&self, result: &RunResult) -> Result<(), ReportError> {
        let markdown = render_markdown(result);
        self.atomic_write(&self.root.join("complete.md"), markdown.as_bytes())?;
        let html = render_html(&markdown);
        self.atomic_write(&self.root.join("complete.html"), html.as_bytes())?;
        Ok(())
    }

    fn atomic_write(&self, path: &Path, bytes: &[u8]) -> Result<(), ReportError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("tmp");
        fs::write(&temp, bytes)?;
        fs::rename(temp, path)?;
        Ok(())
    }
}

fn validate_section(section: &str) -> Result<(), ReportError> {
    let safe = !section.is_empty()
        && section
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if safe {
        Ok(())
    } else {
        Err(ReportError::UnsafeSection(section.into()))
    }
}

fn render_markdown(result: &RunResult) -> String {
    let state = &result.state;
    let mut out = format!(
        "# TradingAgent Report\n\n**Symbol:** {}\n\n**Trade Date:** {}\n\n**Rating:** {}\n",
        state.symbol,
        state.trade_date,
        rating_name(result.rating),
    );
    push_section(&mut out, "Market", state.market_report.as_deref());
    push_section(&mut out, "Sentiment", state.sentiment_report.as_deref());
    push_section(&mut out, "News", state.news_report.as_deref());
    push_section(
        &mut out,
        "Fundamentals",
        state.fundamentals_report.as_deref(),
    );
    push_section(&mut out, "Research", state.research_plan.as_deref());
    push_section(&mut out, "Trader", state.trader_plan.as_deref());
    push_section(&mut out, "Final Decision", state.final_decision.as_deref());
    out
}

fn push_section(out: &mut String, heading: &str, content: Option<&str>) {
    if let Some(content) = content {
        out.push_str(&format!("\n## {heading}\n\n{content}\n"));
    }
}

fn rating_name(rating: FinalRating) -> &'static str {
    match rating {
        FinalRating::Buy => "Buy",
        FinalRating::Overweight => "Overweight",
        FinalRating::Hold => "Hold",
        FinalRating::Underweight => "Underweight",
        FinalRating::Sell => "Sell",
        FinalRating::Review => "Review",
    }
}

fn render_html(markdown: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>TradingAgent Report</title><style>body{{font-family:system-ui,sans-serif;max-width:900px;margin:2rem auto;padding:0 1rem;line-height:1.5}}pre{{white-space:pre-wrap;font:inherit}}</style></head><body><pre>{}</pre></body></html>\n",
        escape_html(markdown)
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl trading_agent_core::events::EventSink for ReportWriter {
    fn emit(
        &self,
        event: &trading_agent_core::events::RunEvent,
    ) -> Result<(), trading_agent_core::CoreError> {
        self.write_event(event)
            .map_err(|error| trading_agent_core::CoreError::Persistence(error.to_string()))
    }
}
