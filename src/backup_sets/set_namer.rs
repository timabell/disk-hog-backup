use super::BACKUP_SET_PREFIX;
use chrono::{Datelike, Timelike, Utc};

pub fn generate_name<F>(get_time: F) -> String
where
	F: Fn() -> chrono::DateTime<Utc>,
{
	let time = get_time();
	format!(
		"{}{:04}{:02}{:02}-{:02}{:02}{:02}",
		BACKUP_SET_PREFIX,
		time.year(),
		time.month(),
		time.day(),
		time.hour(),
		time.minute(),
		time.second()
	)
}

/// Generate a backup set name using the current time.
/// For testing, set DHB_TEST_TIMESTAMP env var to an RFC3339 timestamp
/// (e.g., "2025-01-01T00:00:00Z") to use a fixed time instead.
pub fn generate_backup_set_name() -> String {
	if let Ok(timestamp) = std::env::var("DHB_TEST_TIMESTAMP")
		&& let Ok(time) = chrono::DateTime::parse_from_rfc3339(&timestamp)
	{
		return generate_name(|| time.with_timezone(&Utc));
	}
	generate_name(Utc::now)
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::TimeZone;

	#[test]
	fn test_generates_set_name() {
		let fixed_time = Utc.with_ymd_and_hms(2001, 2, 3, 14, 5, 6).unwrap();
		let name = generate_name(|| fixed_time);
		assert_eq!(name, "dhb-set-20010203-140506");
	}
}
