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

/// The type of an ignore pattern
#[derive(Clone)]
enum PatternType {
	/// Regular pattern (may contain wildcards)
	Regular,
	/// Directory pattern (ends with '/', matches directory and all its contents)
	Directory,
	/// Absolute pattern (starts with '/', matches from root)
	Absolute,
}

/// Represents a single ignore pattern
#[derive(Clone)]
struct IgnorePattern {
	pattern: String,
	pattern_type: PatternType,
	is_negated: bool,
}

impl IgnorePattern {
	fn new(pattern: &str) -> Self {
		// Check if the pattern is negated
		let is_negated = pattern.starts_with('!');
		let pattern = if is_negated {
			pattern[1..].to_string()
		} else {
			pattern.to_string()
		};

		// Determine pattern type
		let pattern_type = if pattern.ends_with('/') {
			PatternType::Directory
		} else if pattern.starts_with('/') {
			PatternType::Absolute
		} else {
			PatternType::Regular
		};

		Self {
			pattern,
			pattern_type,
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

		match self.pattern_type {
			PatternType::Regular => self.matches_regular(&path_str, &filename),
			PatternType::Directory => self.matches_directory(&path_str, path),
			PatternType::Absolute => self.matches_absolute(&path_str),
		}
	}

	/// Match a regular pattern (may contain wildcards)
	fn matches_regular(&self, path_str: &str, filename: &str) -> bool {
		// Check if the pattern contains wildcards
		if self.pattern.contains('*') {
			// With wildcards, check each component
			let components: Vec<&str> = path_str.split('/').collect();

			// Check if any component or the filename matches the pattern
			components
				.iter()
				.any(|component| self.matches_wildcard(component, &self.pattern))
				|| self.matches_wildcard(filename, &self.pattern)
		} else {
			// Without wildcards, check for exact matches
			if path_str == self.pattern {
				return true;
			}

			// Check if path starts with pattern/ (direct child)
			if path_str.starts_with(&format!("{}/", self.pattern)) {
				return true;
			}

			// Check if any path component matches exactly
			let components: Vec<&str> = path_str.split('/').collect();
			if components.contains(&&self.pattern[..]) {
				return true;
			}

			// Check if filename matches exactly
			filename == self.pattern
		}
	}

	/// Match a directory pattern
	fn matches_directory(&self, path_str: &str, path: &Path) -> bool {
		// Remove the trailing slash
		let dir_pattern = &self.pattern[..self.pattern.len() - 1];

		// Check if the pattern contains wildcards
		if dir_pattern.contains('*') {
			// With wildcards, check each component
			let components: Vec<&str> = path_str.split('/').collect();

			// Check if any component matches the wildcard pattern
			for (i, component) in components.iter().enumerate() {
				if self.matches_wildcard(component, dir_pattern) {
					// If matching component isn't the last part, path is inside matching dir
					if i < components.len() - 1 {
						return true;
					}
					// If it's the last component, only match if it's a directory
					else if path.is_dir() {
						return true;
					}
				}
			}
			false
		} else {
			// Without wildcards

			// Exact match for a directory
			if path_str == dir_pattern && path.is_dir() {
				return true;
			}

			// Path inside this directory
			if path_str.starts_with(&format!("{}/", dir_pattern)) {
				return true;
			}

			// Check for matching directory components
			let components: Vec<&str> = path_str.split('/').collect();
			for (i, &component) in components.iter().enumerate() {
				if component == dir_pattern {
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
	}

	/// Match an absolute pattern
	fn matches_absolute(&self, path_str: &str) -> bool {
		// Remove the leading slash
		let abs_pattern = &self.pattern[1..];

		// Exact match or direct child
		path_str == abs_pattern || path_str.starts_with(&format!("{}/", abs_pattern))
	}

	/// Check if a string matches a wildcard pattern
	fn matches_wildcard(&self, s: &str, pattern: &str) -> bool {
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
	}
}

/// Manager for ignore patterns
struct IgnoreManager {
	patterns: Vec<IgnorePattern>,
}

impl IgnoreManager {
	fn new() -> Self {
		let mut manager = Self {
			patterns: Vec::new(),
		};

		// Add built-in default patterns
		manager.add_pattern(".cache/"); // Ignore .cache directories by default

		manager
	}

	fn add_pattern(&mut self, pattern: &str) {
		self.patterns.push(IgnorePattern::new(pattern));
	}

	fn load_patterns_from_file(&mut self, path: &Path) -> io::Result<()> {
		let file = match fs::File::open(path) {
			Ok(file) => file,
			Err(ref e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
			Err(e) => return Err(e),
		};

		let reader = BufReader::new(file);
		for line in reader.lines() {
			let line = line?;
			let line = line.trim();
			if !line.is_empty() && !line.starts_with('#') {
				self.add_pattern(line);
			}
		}

		Ok(())
	}

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
		ignore_manager.load_patterns_from_file(&ignore_file)?;
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
				local_ignore_manager.load_patterns_from_file(&local_ignore_file)?;
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
