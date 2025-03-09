use std::fs;
use std::io;
use std::path::Path;
use std::time::Instant;

use crate::dhcopy::streaming_copy::{BackupContext, copy_file_with_streaming};
use ignore::WalkBuilder;

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

	// Process the files and directories using WalkBuilder to respect .dhbignore
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

/// Process a directory using the ignore crate to respect .dhbignore files
fn process_directory(
	source_path: &Path,
	dest_path: &Path,
	prev_backup: Option<&str>,
	context: &mut BackupContext,
) -> io::Result<()> {
	// Create the destination directory if it doesn't exist
	fs::create_dir_all(dest_path)?;

	// Configure WalkBuilder to use .dhbignore files
	let mut builder = WalkBuilder::new(source_path);

	// Add .dhbignore as a custom ignore file
	builder.add_custom_ignore_filename(".dhbignore");

	// Don't follow symlinks to avoid cycles
	builder.follow_links(false);

	// ignore crate comes with set of default ignores that we *reall* don't want active for a *backup* tool
	builder.standard_filters(false);

	// Process each entry in the walk
	let walker = builder.build();

	for result in walker {
		match result {
			Ok(entry) => {
				let entry_path = entry.path();

				// Skip the root directory itself
				if entry_path == source_path {
					continue;
				}

				// Get the relative path from the source directory
				let rel_path = entry_path
					.strip_prefix(source_path)
					.expect("Entry should be prefixed by source path");

				// Construct the destination path
				let entry_dest_path = dest_path.join(rel_path);

				// Determine the previous backup path if available
				let prev_backup_path = prev_backup.map(|p| Path::new(p).join(rel_path));

				if entry_path.is_symlink() {
					let target = fs::read_link(entry_path)?;

					#[cfg(unix)]
					symlink(&target, &entry_dest_path)?;

					#[cfg(windows)]
					if target.is_dir() {
						symlink_dir(&target, &entry_dest_path)?;
					} else {
						symlink_file(&target, &entry_dest_path)?;
					}
				} else if entry_path.is_dir() {
					fs::create_dir_all(&entry_dest_path)?;
				} else {
					// Ensure parent directories exist
					if let Some(parent) = entry_dest_path.parent() {
						fs::create_dir_all(parent)?;
					}

					// Output the full path of the file being processed
					println!("Processing: {}", entry_path.display());

					// Copy the file with streaming
					copy_file_with_streaming(
						entry_path,
						&entry_dest_path,
						prev_backup_path.as_deref(),
						rel_path,
						context,
					)?;
				}
			}
			Err(err) => {
				eprintln!("Error walking directory: {}", err);
				return Err(io::Error::new(io::ErrorKind::Other, err));
			}
		}
	}

	Ok(())
}
