use chrono::{DateTime, Utc};
use rand::Rng;
use std::env;
use std::fs::{self};
use std::io::{self};

pub fn create_tmp_folder(prefix: &str) -> io::Result<String> {
	let mut rng = rand::rng();
	let random_suffix: u32 = rng.random();
	let dir = env::temp_dir().join(format!("dhb-{}-{}", prefix, random_suffix));
	fs::create_dir_all(&dir)?;
	Ok(dir.to_string_lossy().into_owned())
}

// Returns a function that always returns the same time
pub fn time_fixer() -> impl Fn() -> DateTime<Utc> {
	let fixed_time = Utc::now();
	move || fixed_time
}
