use chrono::{Datelike, Timelike, Utc};

pub fn generate_name<F>(get_time: F) -> String
where
	F: Fn() -> chrono::DateTime<Utc>,
{
	let time = get_time();
	format!(
		"dhb-set-{:04}{:02}{:02}-{:02}{:02}{:02}",
		time.year(),
		time.month(),
		time.day(),
		time.hour(),
		time.minute(),
		time.second()
	)
}

/// Generate a backup set name using the current time
pub fn generate_backup_set_name() -> String {
	generate_name(|| Utc::now())
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
