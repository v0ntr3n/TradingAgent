use crate::agents::AgentReport;

use super::FinalRating;

pub(crate) fn append_debate_round(
    transcript: &mut String,
    round: u32,
    bull: &AgentReport,
    bear: &AgentReport,
) {
    if !transcript.is_empty() {
        transcript.push_str("\n\n");
    }
    transcript.push_str(&format!(
        "Round {round}\nBull researcher:\n{}\n\nBear researcher:\n{}",
        bull.content, bear.content
    ));
}

pub(crate) fn parse_final_rating(output: &str) -> FinalRating {
    for line in output.lines() {
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        let label = label.trim().trim_matches('*').trim();
        if !label.eq_ignore_ascii_case("rating") {
            continue;
        }

        let value = value.trim().trim_matches('*').trim();
        if value.eq_ignore_ascii_case("buy") {
            return FinalRating::Buy;
        }
        if value.eq_ignore_ascii_case("overweight") {
            return FinalRating::Overweight;
        }
        if value.eq_ignore_ascii_case("hold") {
            return FinalRating::Hold;
        }
        if value.eq_ignore_ascii_case("underweight") {
            return FinalRating::Underweight;
        }
        if value.eq_ignore_ascii_case("sell") {
            return FinalRating::Sell;
        }
        return FinalRating::Review;
    }

    FinalRating::Review
}
