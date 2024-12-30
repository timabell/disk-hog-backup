use chrono::{TimeZone, Utc};
use std::sync::Mutex;

fn generate_name<F>(get_time: F) -> String
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_generates_set_name() {
        let fixed_time = Utc.ymd(2001, 2, 3).and_hms(14, 5, 6);
        let name = generate_name(|| fixed_time);
        assert_eq!(name, "dhb-set-20010203-140506");
    }
}
