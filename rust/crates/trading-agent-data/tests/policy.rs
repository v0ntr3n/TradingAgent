use chrono::{TimeZone, Utc};
use chrono_tz::{Asia::Bangkok, UTC};
use trading_agent_data::{DataAccess, DataPolicy, DataStatus, SourceAvailability, VendorRouter};

#[test]
fn current_only_data_is_allowed_for_the_runtime_local_date() {
    let now = Utc.with_ymd_and_hms(2026, 10, 6, 7, 0, 0).unwrap();
    let as_of = chrono::NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();
    assert_eq!(
        DataPolicy::classify(as_of, SourceAvailability::CurrentOnly, now, Bangkok),
        DataAccess::Allowed
    );
}

#[test]
fn current_only_data_is_withheld_for_historical_dates() {
    let now = Utc.with_ymd_and_hms(2026, 10, 6, 7, 0, 0).unwrap();
    let as_of = chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
    assert_eq!(
        DataPolicy::classify(as_of, SourceAvailability::CurrentOnly, now, Bangkok),
        DataAccess::WithheldHistorical
    );
}

#[test]
fn runtime_timezone_controls_date_rollover() {
    let now = Utc.with_ymd_and_hms(2026, 10, 6, 18, 30, 0).unwrap();
    let as_of = chrono::NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();

    assert_eq!(
        DataPolicy::classify(as_of, SourceAvailability::CurrentOnly, now, Bangkok),
        DataAccess::WithheldHistorical
    );
    assert_eq!(
        DataPolicy::classify(as_of, SourceAvailability::CurrentOnly, now, UTC),
        DataAccess::Allowed
    );
}

#[test]
fn archival_sources_remain_allowed_for_historical_dates() {
    let now = Utc.with_ymd_and_hms(2026, 10, 6, 7, 0, 0).unwrap();
    let as_of = chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    assert_eq!(
        DataPolicy::classify(as_of, SourceAvailability::Archival, now, Bangkok),
        DataAccess::Allowed
    );
}

#[test]
fn data_status_distinguishes_unavailable_from_historical_withholding() {
    let unavailable: DataStatus<String> = DataStatus::Unavailable {
        source: "coindesk".into(),
        reason: "rate limited".into(),
    };
    let withheld: DataStatus<String> = DataStatus::WithheldHistorical {
        source: "binance-live".into(),
        as_of: chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
    };
    assert_ne!(unavailable, withheld);
}

#[test]
fn vendor_router_preserves_exact_ordered_fallback_chain() {
    let router = VendorRouter::new([
        ("news", "coindesk,coinstats,blockbeats"),
        ("market", "binance"),
    ])
    .unwrap();

    assert_eq!(
        router.chain("news").unwrap(),
        ["coindesk", "coinstats", "blockbeats"]
    );
    assert_eq!(router.chain("market").unwrap(), ["binance"]);
    assert!(router.chain("fundamentals").is_err());
}

#[test]
fn vendor_router_rejects_empty_or_duplicate_entries() {
    assert!(VendorRouter::new([("news", "")]).is_err());
    assert!(VendorRouter::new([("news", "coindesk,,coinstats")]).is_err());
    assert!(VendorRouter::new([("news", "coindesk,coindesk")]).is_err());
}
