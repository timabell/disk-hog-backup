use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

use crate::dhcopy::streaming_copy::{BackupContext, copy_file_with_streaming};

#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

/// Performs a backup of a folder with MD5-based hardlinking optimization
pub fn backup_folder(source: &str, dest: &str, prev_backup: Option<&str>) -> io::Result<()> {
	println!("backing up folder {} into {}", source, dest);

	// Start timing the backup process
	let start_time = Instant::now();

	// Create or initialize the backup context once at the top level
	let dest_path = Path::new(dest);
	let mut context = if let Some(prev) = prev_backup {
		BackupContext::with_previous_backup(dest_path, Path::new(prev))?
	} else {
		BackupContext::new(dest_path)
	};

	// Process the files and directories using our custom ignore implementation
	let source_path = Path::new(source);
	process_directory(source_path, dest_path, prev_backup, &mut context)?;

	// Save the MD5 store
	context.save_md5_store()?;

	// Calculate and display the total time taken
	let duration = start_time.elapsed();
	let total_seconds = duration.as_secs();
	let hours = total_seconds / 3600;
	let minutes = (total_seconds % 3600) / 60;
	let seconds = total_seconds % 60;

	// Format the time as hours, minutes, seconds
	if hours > 0 {
		println!(
			"Backup completed in {} hours {} mins {} seconds",
			hours, minutes, seconds
		);
	} else if minutes > 0 {
		println!("Backup completed in {} mins {} seconds", minutes, seconds);
	} else {
		println!("Backup completed in {} seconds", seconds);
	}

	Ok(())
}

/// A pattern for ignoring files
#[derive(Clone)]
struct IgnorePattern {
	pattern: String,
	is_negated: bool,
}

impl IgnorePattern {
	fn new(pattern: &str) -> Self {
		let is_negated = pattern.starts_with('!');
		let pattern = if is_negated {
			pattern[1..].to_string()
		} else {
			pattern.to_string()
		};

		Self {
			pattern,
			is_negated,
		}
	}

	/// Check if a path matches this pattern
	fn matches(&self, path: &Path, base_dir: &Path) -> bool {
		// Get the relative path from the base directory
		let rel_path = match path.strip_prefix(base_dir) {
			Ok(p) => p,
			Err(_) => return false,
		};

		let path_str = rel_path.to_string_lossy();

		// Simple glob matching
		if self.pattern.starts_with("*") {
			// *.ext pattern
			let suffix = &self.pattern[1..];
			path_str.ends_with(suffix)
		} else if self.pattern.ends_with("/") {
			// directory/ pattern
			let dir_name = &self.pattern[..self.pattern.len() - 1];
			path.is_dir()
				&& (path_str == dir_name || path_str.starts_with(&format!("{}/", dir_name)))
		} else {
			// Exact match or prefix match for directories
			path_str == self.pattern
				|| (path.is_dir() && path_str.starts_with(&format!("{}/", self.pattern)))
		}
	}
}

/// Manager for ignore patterns
struct IgnoreManager {
	patterns: Vec<IgnorePattern>,
}

impl IgnoreManager {
	fn new() -> Self {
		Self {
			patterns: Vec::new(),
		}
	}

	/// Load patterns from a .dhbignore file
	fn load_from_file(&mut self, ignore_file: &Path) -> io::Result<()> {
		if !ignore_file.exists() {
			return Ok(());
		}

		let file = fs::File::open(ignore_file)?;
		let reader = BufReader::new(file);

		for line in reader.lines() {
			let line = line?;
			let line = line.trim();

			// Skip empty lines and comments
			if line.is_empty() || line.starts_with('#') {
				continue;
			}

			self.patterns.push(IgnorePattern::new(line));
		}

		Ok(())
	}

	/// Check if a path should be ignored
	fn should_ignore(&self, path: &Path, base_dir: &Path) -> bool {
		let mut should_ignore = false;

		for pattern in &self.patterns {
			if pattern.matches(path, base_dir) {
				// If it's a negated pattern, we explicitly include it
				if pattern.is_negated {
					return false;
				}
				// Otherwise, mark it for ignoring
				should_ignore = true;
			}
		}

		should_ignore
	}
}

/// Process a directory using our custom ignore implementation
fn process_directory(
	source_path: &Path,
	dest_path: &Path,
	prev_backup: Option<&str>,
	context: &mut BackupContext,
) -> io::Result<()> {
	// Create the destination directory if it doesn't exist
	fs::create_dir_all(dest_path)?;

	// Load ignore patterns
	let mut ignore_manager = IgnoreManager::new();
	let ignore_file = source_path.join(".dhbignore");
	if ignore_file.exists() {
		ignore_manager.load_from_file(&ignore_file)?;
	}

	// Process the directory recursively
	process_directory_recursive(
		source_path,
		source_path,
		dest_path,
		prev_backup,
		context,
		&ignore_manager,
	)
}

/// Recursively process a directory, respecting ignore patterns
fn process_directory_recursive(
	base_path: &Path,
	current_path: &Path,
	dest_path: &Path,
	prev_backup: Option<&str>,
	context: &mut BackupContext,
	ignore_manager: &IgnoreManager,
) -> io::Result<()> {
	// Read the directory entries
	for entry in fs::read_dir(current_path)? {
		let entry = entry?;
		let entry_path = entry.path();

		// Check if this entry should be ignored
		if ignore_manager.should_ignore(&entry_path, base_path) {
			println!("Ignoring: {}", entry_path.display());
			continue;
		}

		// Get the relative path from the base directory
		let rel_path = entry_path
			.strip_prefix(base_path)
			.expect("Entry should be prefixed by base path");

		// Construct the destination path
		let entry_dest_path = dest_path.join(rel_path);

		// Determine the previous backup path if available
		let prev_backup_path = prev_backup.map(|p| Path::new(p).join(rel_path));

		if entry_path.is_symlink() {
			let target = fs::read_link(&entry_path)?;

			#[cfg(unix)]
			symlink(&target, &entry_dest_path)?;

			#[cfg(windows)]
			if target.is_dir() {
				symlink_dir(&target, &entry_dest_path)?;
			} else {
				symlink_file(&target, &entry_dest_path)?;
			}
		} else if entry_path.is_dir() {
			// Create the directory in the destination
			fs::create_dir_all(&entry_dest_path)?;

			// Check for a .dhbignore file in this directory
			let local_ignore_file = entry_path.join(".dhbignore");
			let mut local_ignore_manager = IgnoreManager::new();

			// Start with the parent ignore patterns
			local_ignore_manager.patterns = ignore_manager.patterns.clone();

			// Add any local ignore patterns
			if local_ignore_file.exists() {
				local_ignore_manager.load_from_file(&local_ignore_file)?;
			}

			// Process this directory recursively
			process_directory_recursive(
				base_path,
				&entry_path,
				dest_path,
				prev_backup,
				context,
				&local_ignore_manager,
			)?;
		} else {
			// Ensure parent directories exist
			if let Some(parent) = entry_dest_path.parent() {
				fs::create_dir_all(parent)?;
			}

			// Output the full path of the file being processed
			println!("Processing: {}", entry_path.display());

			// Copy the file with streaming
			copy_file_with_streaming(
				&entry_path,
				&entry_dest_path,
				prev_backup_path.as_deref(),
				rel_path,
				context,
			)?;
		}
	}

	Ok(())
}
