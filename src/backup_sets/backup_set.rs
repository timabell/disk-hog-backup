use super::BACKUP_SET_PREFIX;
use std::fs::{self, DirEntry};
use std::io;
use std::path::Path;

/// Returns backup set directories from the destination folder
pub fn backup_sets(dest: &str) -> io::Result<Vec<DirEntry>> {
	let entries: Vec<_> = fs::read_dir(dest)?
		.filter_map(Result::ok)
		.filter(|entry| {
			entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
				&& entry
					.file_name()
					.to_string_lossy()
					.starts_with(BACKUP_SET_PREFIX)
		})
		.collect();
	Ok(entries)
}

pub fn find_most_recent_set(dest: &str) -> Option<String> {
	let mut sets: Vec<_> = backup_sets(dest)
		.ok()?
		.into_iter()
		.filter_map(|entry| {
			let created = entry
				.metadata()
				.ok()?
				.created()
				.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
			Some((entry.file_name().to_string_lossy().to_string(), created))
		})
		.collect();

	sets.sort_by_key(|(_, created)| *created);
	sets.last().map(|(name, _)| name.clone())
}

/// Find the most recent backup set and return its full path
pub fn find_most_recent_backup_set(dest: &str) -> Option<String> {
	find_most_recent_set(dest)
		.map(|set_name| Path::new(dest).join(set_name).to_string_lossy().to_string())
}
