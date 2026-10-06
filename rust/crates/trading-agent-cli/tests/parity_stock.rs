mod support;

use support::{ParityFixture, assert_fixture, run_fixture};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stock_fixture_matches_python_v06_semantics() {
    let fixture: ParityFixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/parity/stock_run.json"
    )).unwrap();
    let (result, records) = run_fixture(&fixture).await;
    assert_fixture(&fixture, &result, &records);
}
