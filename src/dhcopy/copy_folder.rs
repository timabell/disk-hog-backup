use std::collections::HashSet;
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

	// Track ignored paths
	let mut ignored_paths = HashSet::new();

	// Process the files and directories using our custom ignore implementation
	let source_path = Path::new(source);
	process_directory(
		source_path,
		dest_path,
		prev_backup,
		&mut context,
		&mut ignored_paths,
	)?;

	// Save the MD5 store
	context.save_md5_store()?;

	// Output summary of ignored paths
	if !ignored_paths.is_empty() {
		println!("\nIgnored paths summary:");
		let mut sorted_paths: Vec<_> = ignored_paths.iter().collect();
		sorted_paths.sort();
		for path in sorted_paths {
			println!("  {}", path);
		}
		println!("Total ignored paths: {}", ignored_paths.len());
	}

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
		let filename = path
			.file_name()
			.map(|f| f.to_string_lossy())
			.unwrap_or_default();

		// Helper function to check if a string matches a simple wildcard pattern
		let matches_wildcard = |s: &str, pattern: &str| -> bool {
			if !pattern.contains('*') {
				return s == pattern;
			}

			// Split the pattern by * and check if the string contains all parts in order
			let parts: Vec<&str> = pattern.split('*').collect();
			if parts.is_empty() {
				return true; // Pattern is just "*"
			}

			let mut remaining = s;

			// Check if the pattern starts with a non-* character
			if !pattern.starts_with('*') && !remaining.starts_with(parts[0]) {
				return false;
			}

			// Check if the pattern ends with a non-* character
			if !pattern.ends_with('*') && !remaining.ends_with(parts[parts.len() - 1]) {
				return false;
			}

			// Check all parts in order
			for part in parts {
				if part.is_empty() {
					continue; // Skip empty parts (consecutive *'s)
				}

				match remaining.find(part) {
					Some(pos) => {
						// Move past this part in the remaining string
						remaining = &remaining[pos + part.len()..];
					}
					None => return false,
				}
			}

			true
		};

		// Simple glob matching
		if self.pattern.starts_with("*") && self.pattern.ends_with("*") {
			// *substring* pattern
			let substring = &self.pattern[1..self.pattern.len() - 1];
			path_str.contains(substring) || filename.contains(substring)
		} else if self.pattern.starts_with("*") {
			// *.ext pattern
			let suffix = &self.pattern[1..];
			path_str.ends_with(suffix) || filename.ends_with(suffix)
		} else if self.pattern.ends_with("*") {
			// prefix* pattern
			let prefix = &self.pattern[..self.pattern.len() - 1];
			path_str.starts_with(prefix) || filename.starts_with(prefix)
		} else if self.pattern.ends_with("/") {
			// directory/ pattern - match the directory and all its contents at any depth
			let dir_pattern = &self.pattern[..self.pattern.len() - 1];

			// Check if the pattern contains wildcards
			if dir_pattern.contains('*') {
				// Split the path into components
				let components: Vec<&str> = path_str.split('/').collect();

				// Check if any component matches the wildcard pattern
				for (i, component) in components.iter().enumerate() {
					if matches_wildcard(component, dir_pattern) {
						// If this component matches and it's not the last component,
						// then the path is inside a directory that matches the pattern
						if i < components.len() - 1 {
							return true;
						}
						// If it's the last component, it matches only if it's a directory
						else if path.is_dir() {
							return true;
						}
					}
				}

				false
			} else {
				// Regular directory pattern without wildcards
				let dir_name = dir_pattern;

				// Check if path is exactly this directory
				if path_str == dir_name && path.is_dir() {
					return true;
				}

				// Check if path starts with this directory name followed by a slash
				// This handles direct children
				if path_str.starts_with(&format!("{}/", dir_name)) {
					return true;
				}

				// Check if any path component matches the directory name
				// This handles nested directories with the same name
				let components: Vec<&str> = path_str.split('/').collect();
				for (i, &component) in components.iter().enumerate() {
					if component == dir_name {
						// If this component matches and it's not the last component,
						// then the path is inside a directory that matches the pattern
						if i < components.len() - 1 {
							return true;
						}
						// If it's the last component, it matches only if it's a directory
						else if path.is_dir() {
							return true;
						}
					}
				}

				false
			}
		} else if self.pattern.starts_with("/") {
			// Absolute pattern (only matches at root level)
			let pattern = &self.pattern[1..];
			path_str == pattern || path_str.starts_with(&format!("{}/", pattern))
		} else if self.pattern.contains('*') {
			// Pattern with wildcards
			// Split the path into components
			let components: Vec<&str> = path_str.split('/').collect();

			// Check if any component matches the wildcard pattern
			components
				.iter()
				.any(|component| matches_wildcard(component, &self.pattern))
				|| matches_wildcard(&filename, &self.pattern)
		} else {
			// Exact match at any level
			if path_str == self.pattern {
				return true;
			}

			// Check if it's a direct child of the base directory
			if path_str.starts_with(&format!("{}/", self.pattern)) {
				return true;
			}

			// Check if any path component matches exactly
			let components: Vec<&str> = path_str.split('/').collect();
			if components.contains(&self.pattern.as_str()) {
				return true;
			}

			// Check if filename matches
			filename == self.pattern
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
	ignored_paths: &mut HashSet<String>,
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
		ignored_paths,
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
	ignored_paths: &mut HashSet<String>,
) -> io::Result<()> {
	// Read the directory entries
	for entry in fs::read_dir(current_path)? {
		let entry = entry?;
		let entry_path = entry.path();

		// Check if this entry should be ignored
		if ignore_manager.should_ignore(&entry_path, base_path) {
			println!("Ignoring: {}", entry_path.display());
			ignored_paths.insert(entry_path.display().to_string());
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

		// Skip named pipes (FIFOs) and other special files
		#[cfg(unix)]
		{
			use std::os::unix::fs::FileTypeExt;
			let file_type = entry.file_type()?;
			if file_type.is_fifo()
				|| file_type.is_socket()
				|| file_type.is_block_device()
				|| file_type.is_char_device()
			{
				println!(
					"Skipping special file: {} (type: {:?})",
					entry_path.display(),
					file_type
				);
				ignored_paths.insert(entry_path.display().to_string());
				continue;
			}
		}

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
				ignored_paths,
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
