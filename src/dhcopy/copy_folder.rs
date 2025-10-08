use bytesize::ByteSize;
use std::collections::HashSet;
use std::fs;
use std::io::{self};
use std::path::Path;

use crate::dhcopy::streaming_copy::{BackupContext, copy_file_with_streaming};

#[cfg(unix)]
use std::os::unix::fs::symlink;

use crate::dhcopy::ignore_patterns::IgnoreManager;
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

/// Calculate the total size of files in a directory (respecting ignore patterns)
fn calculate_total_size(source_path: &Path) -> io::Result<u64> {
	let mut total_size = 0u64;
	let mut ignore_manager = IgnoreManager::new();

	// Load ignore patterns from the source directory
	let ignore_file = source_path.join(".dhbignore");
	if ignore_file.exists() {
		ignore_manager.load_patterns_from_file(&ignore_file)?;
	}

	calculate_directory_size_recursive(source_path, source_path, &ignore_manager, &mut total_size)?;

	Ok(total_size)
}

/// Recursively calculate directory size
fn calculate_directory_size_recursive(
	base_path: &Path,
	current_path: &Path,
	ignore_manager: &IgnoreManager,
	total_size: &mut u64,
) -> io::Result<()> {
	for entry in fs::read_dir(current_path)? {
		let entry = entry?;
		let entry_path = entry.path();

		// Skip ignored paths
		if ignore_manager.should_ignore(&entry_path, base_path) {
			continue;
		}

		// Skip special files on Unix
		#[cfg(unix)]
		{
			use std::os::unix::fs::FileTypeExt;
			let file_type = entry.file_type()?;
			if file_type.is_fifo()
				|| file_type.is_socket()
				|| file_type.is_block_device()
				|| file_type.is_char_device()
			{
				continue;
			}
		}

		if entry_path.is_symlink() {
			// Skip symlinks (we don't count their target size)
			continue;
		} else if entry_path.is_dir() {
			// Check for local .dhbignore
			let local_ignore_file = entry_path.join(".dhbignore");
			let mut local_ignore_manager = IgnoreManager::new();
			local_ignore_manager.patterns = ignore_manager.patterns.clone();

			if local_ignore_file.exists() {
				local_ignore_manager.load_patterns_from_file(&local_ignore_file)?;
			}

			// Recurse into directory
			calculate_directory_size_recursive(
				base_path,
				&entry_path,
				&local_ignore_manager,
				total_size,
			)?;
		} else {
			// Regular file - add its size
			if let Ok(metadata) = entry_path.metadata() {
				*total_size += metadata.len();
			}
		}
	}

	Ok(())
}

/// Performs a backup of a folder with MD5-based hardlinking optimization
pub fn backup_folder(
	source: &str,
	dest: &str,
	prev_backup: Option<&str>,
	session_id: &str,
	initial_disk_space: Option<crate::disk_space::DiskSpace>,
) -> io::Result<()> {
	eprintln!("backing up folder {} into {}", source, dest);

	// Calculate total size for progress tracking
	let source_path = Path::new(source);
	eprintln!("Calculating total size...");
	let size_calc_start = std::time::Instant::now();
	let total_size = calculate_total_size(source_path)?;
	let size_calc_duration = size_calc_start.elapsed();
	eprintln!(
		"Total size: {} ({} bytes) - calculated in {:.2}s",
		ByteSize(total_size),
		total_size,
		size_calc_duration.as_secs_f64()
	);

	// Create or initialize the backup context once at the top level
	let dest_path = Path::new(dest);
	let mut context = if let Some(prev) = prev_backup {
		BackupContext::with_previous_backup(
			dest_path,
			Path::new(prev),
			session_id,
			total_size,
			size_calc_duration,
			initial_disk_space,
		)?
	} else {
		BackupContext::new(
			dest_path,
			session_id,
			total_size,
			size_calc_duration,
			initial_disk_space,
		)
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

	// Output summary of ignored paths
	if !ignored_paths.is_empty() {
		context.stats.clear_progress_line();
		eprintln!("\nIgnored paths summary:");
		let mut sorted_paths: Vec<_> = ignored_paths.iter().collect();
		sorted_paths.sort();
		for path in sorted_paths {
			eprintln!("  {}", path);
		}
		eprintln!("Total ignored paths: {}", ignored_paths.len());
	}

	// Clear the progress display before showing final summary
	context.stats.clear_progress_display();

	// Capture final disk space before saving stats
	// dest_path is the backup set path, we need the parent (backup root) for disk space
	if let Some(backup_root) = dest_path.parent()
		&& let Ok(final_disk_space) = crate::disk_space::get_disk_space(backup_root)
	{
		context.stats.set_final_disk_space(final_disk_space);
	}

	// Save the MD5 store and backup statistics
	context.save_md5_store()?;
	context.save_stats()?;
	context.print_stats_summary();

	Ok(())
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
			context.stats.clear_progress_line();
			eprintln!("Ignoring: {}", entry_path.display());
			context.stats.update_progress_display();
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
				context.stats.clear_progress_line();
				eprintln!(
					"Skipping special file: {} (type: {:?})",
					entry_path.display(),
					file_type
				);
				context.stats.update_progress_display();
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

			// Output the full path of the file being processed with size
			let file_size = entry_path.metadata()?.len();
			context.stats.clear_progress_line();
			eprintln!(
				"Processing: {} ({})",
				entry_path.display(),
				ByteSize(file_size)
			);
			context.stats.update_progress_display();

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

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[test]
	fn test_calculate_total_size() -> io::Result<()> {
		let temp_dir = tempdir()?;
		let source = temp_dir.path();

		// Create some test files
		fs::write(source.join("file1.txt"), "hello")?; // 5 bytes
		fs::write(source.join("file2.txt"), "world!!")?; // 7 bytes
		fs::create_dir(source.join("subdir"))?;
		fs::write(source.join("subdir/file3.txt"), "test")?; // 4 bytes

		let total = calculate_total_size(source)?;
		assert_eq!(total, 16, "Should calculate total size of all files");

		Ok(())
	}

	#[test]
	fn test_calculate_total_size_respects_ignore() -> io::Result<()> {
		let temp_dir = tempdir()?;
		let source = temp_dir.path();

		// Create test files
		fs::write(source.join("keep.txt"), "keep")?; // 4 bytes
		fs::write(source.join("ignore.log"), "ignore this")?; // 11 bytes (should be ignored)

		// Create .dhbignore to ignore .log files
		fs::write(source.join(".dhbignore"), "*.log\n")?; // 6 bytes

		let total = calculate_total_size(source)?;
		// Total should be: keep.txt (4) + .dhbignore (6) = 10
		// ignore.log should be excluded
		assert_eq!(
			total, 10,
			"Should count keep.txt and .dhbignore but not ignore.log"
		);

		Ok(())
	}
}
