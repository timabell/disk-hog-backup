use std::io::{BufRead, BufReader};
use std::path::Path;
use std::{fs, io};

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
pub(crate) struct IgnorePattern {
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
pub(crate) struct IgnoreManager {
	pub(crate) patterns: Vec<IgnorePattern>,
}

impl IgnoreManager {
	pub(crate) fn new() -> Self {
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

	pub(crate) fn load_patterns_from_file(&mut self, path: &Path) -> io::Result<()> {
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

	pub(crate) fn should_ignore(&self, path: &Path, base_dir: &Path) -> bool {
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
