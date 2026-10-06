use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;

use crate::{DataAccess, DataError, DataStatus, SourceAvailability};

pub struct DataPolicy;

impl DataPolicy {
    pub fn classify(
        as_of: NaiveDate,
        availability: SourceAvailability,
        now: DateTime<Utc>,
        timezone: Tz,
    ) -> DataAccess {
        match availability {
            SourceAvailability::Archival => DataAccess::Allowed,
            SourceAvailability::CurrentOnly => {
                if as_of < now.with_timezone(&timezone).date_naive() {
                    DataAccess::WithheldHistorical
                } else {
                    DataAccess::Allowed
                }
            }
        }
    }

    pub fn guard<T, F>(
        source: &str,
        as_of: NaiveDate,
        availability: SourceAvailability,
        now: DateTime<Utc>,
        timezone: Tz,
        fetch: F,
    ) -> Result<DataStatus<T>, DataError>
    where
        F: FnOnce() -> Result<T, DataError>,
    {
        if Self::classify(as_of, availability, now, timezone) == DataAccess::WithheldHistorical {
            return Ok(DataStatus::WithheldHistorical {
                source: source.to_owned(),
                as_of,
            });
        }

        fetch().map(DataStatus::Available)
    }
}
