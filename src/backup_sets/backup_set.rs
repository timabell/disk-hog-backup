use std::fs;
use std::path::Path;

pub fn find_most_recent_set(dest: &str) -> Option<String> {
	match fs::read_dir(dest) {
		Ok(entries) => {
			let mut backup_sets: Vec<_> = entries
				.filter_map(Result::ok)
				.filter(|entry| {
					entry.path().is_dir()
						&& entry.file_name().to_string_lossy().starts_with("dhb-set-")
				})
				.collect();

			if backup_sets.is_empty() {
				return None;
			}

			// Sort by creation time, most recent last
			backup_sets.sort_by_key(|entry| {
				entry
					.path()
					.metadata()
					.map(|meta| {
						meta.created()
							.unwrap_or_else(|_| std::time::SystemTime::UNIX_EPOCH)
					})
					.unwrap_or_else(|_| std::time::SystemTime::UNIX_EPOCH)
			});

			// Return the most recent (last) entry
			backup_sets
				.last()
				.map(|entry| entry.file_name().to_string_lossy().to_string())
		}
		Err(_) => None,
	}
}

/// Find the most recent backup set and return its full path
pub fn find_most_recent_backup_set(dest: &str) -> Option<String> {
	find_most_recent_set(dest)
		.map(|set_name| Path::new(dest).join(set_name).to_string_lossy().to_string())
}
