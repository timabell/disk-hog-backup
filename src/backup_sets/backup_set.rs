use crate::backup_sets::set_namer::generate_name;
use chrono::Utc;
use std::fs;
use std::path::Path;

pub fn create_empty_set<F>(dest: &str, get_time: F) -> Result<String, std::io::Error>
where
	F: Fn() -> chrono::DateTime<Utc>,
{
	let set_name = generate_name(get_time);
	let dir_path = Path::new(dest).join(&set_name);
	fs::create_dir_all(&dir_path)?;
	Ok(set_name)
}
