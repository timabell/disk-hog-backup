use std::fs;
use std::path::Path;

pub fn find_most_recent_set(dest: &str) -> Option<String> {
	match fs::read_dir(dest) {
		Ok(entries) => {
			let mut backup_sets: Vec<_> = entries
				.filter_map(Result::ok)
				.filter_map(|entry| {
					let meta = entry.metadata().ok()?;
					if meta.is_dir() && entry.file_name().to_string_lossy().starts_with("dhb-set-")
					{
						Some((
							entry.file_name().to_string_lossy().to_string(),
							meta.created().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
						))
					} else {
						None
					}
				})
				.collect();

			if backup_sets.is_empty() {
				return None;
			}

			// Sort by creation time, most recent last
			backup_sets.sort_by_key(|(_, created)| *created);

			// Return the most recent (last) entry
			backup_sets.last().map(|(set_name, _)| set_name).cloned()
		}
		Err(_) => None,
	}
}

/// Find the most recent backup set and return its full path
pub fn find_most_recent_backup_set(dest: &str) -> Option<String> {
	find_most_recent_set(dest)
		.map(|set_name| Path::new(dest).join(set_name).to_string_lossy().to_string())
}
