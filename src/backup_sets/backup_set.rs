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
		.map(|entry| entry.file_name().to_string_lossy().to_string())
		.collect();

	// Sort by folder name - works because format dhb-set-YYYYMMDD-HHMMSS sorts chronologically
	sets.sort();
	sets.last().cloned()
}

/// Find the most recent backup set and return its full path
pub fn find_most_recent_backup_set(dest: &str) -> Option<String> {
	find_most_recent_set(dest)
		.map(|set_name| Path::new(dest).join(set_name).to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempfile::TempDir;

	#[test]
	fn find_most_recent_set_uses_folder_name_not_metadata() {
		let temp_dir = TempDir::new().unwrap();
		let dest = temp_dir.path().to_str().unwrap();

		// Create folders in reverse chronological order (oldest created last on filesystem)
		// This ensures the test fails if metadata is used instead of folder name
		let older_set = "dhb-set-20240101-120000";
		let newer_set = "dhb-set-20240615-090000";

		// Create newer set first (so it has older filesystem metadata)
		fs::create_dir(temp_dir.path().join(newer_set)).unwrap();
		// Small delay to ensure different filesystem timestamps
		std::thread::sleep(std::time::Duration::from_millis(10));
		// Create older set second (so it has newer filesystem metadata)
		fs::create_dir(temp_dir.path().join(older_set)).unwrap();

		let result = find_most_recent_set(dest);

		// Should return the set with the newest NAME, not newest filesystem metadata
		assert_eq!(result, Some(newer_set.to_string()));
	}

	#[test]
	fn find_most_recent_set_returns_none_when_no_sets() {
		let temp_dir = TempDir::new().unwrap();
		let dest = temp_dir.path().to_str().unwrap();

		let result = find_most_recent_set(dest);

		assert_eq!(result, None);
	}

	#[test]
	fn find_most_recent_set_ignores_non_backup_folders() {
		let temp_dir = TempDir::new().unwrap();
		let dest = temp_dir.path().to_str().unwrap();

		fs::create_dir(temp_dir.path().join("some-other-folder")).unwrap();
		fs::create_dir(temp_dir.path().join("dhb-set-20240101-120000")).unwrap();

		let result = find_most_recent_set(dest);

		assert_eq!(result, Some("dhb-set-20240101-120000".to_string()));
	}
}
