mod support;

use support::{ParityFixture, assert_fixture, run_fixture};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_crypto_fixture_preserves_v06_behavior_plus_approved_fundamentals_extension() {
    let fixture: ParityFixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/parity/crypto_run.json"
    )).unwrap();
    let (result, records) = run_fixture(&fixture).await;
    assert_fixture(&fixture, &result, &records);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn historical_crypto_fixture_withholds_current_only_vendor_calls() {
    let fixture: ParityFixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/parity/historical_crypto_run.json"
    )).unwrap();
    let (result, records) = run_fixture(&fixture).await;
    assert_fixture(&fixture, &result, &records);
}
