use std::fs;
use std::io;
use std::path::Path;

use crate::dhcopy::streaming_copy::{BackupContext, copy_file_with_streaming};

#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

/// Performs a backup of a folder with MD5-based hardlinking optimization
pub fn backup_folder(source: &str, dest: &str, prev_backup: Option<&str>) -> io::Result<()> {
	println!("backing up folder {} into {}", source, dest);

	// Create or initialize the backup context once at the top level
	let dest_path = Path::new(dest);
	let mut context = if let Some(prev) = prev_backup {
		BackupContext::with_previous_backup(dest_path, Path::new(prev))?
	} else {
		BackupContext::new(dest_path)
	};

	// Process the files and directories
	copy_folder(source, dest, prev_backup, Path::new(""), &mut context)?;

	// Save the MD5 store
	context.save_md5_store()?;

	Ok(())
}

/// Copies a folder with its contents, using hardlinking for unchanged files
pub fn copy_folder(
	source: &str,
	dest: &str,
	prev_backup: Option<&str>,
	rel_path: &Path,
	context: &mut BackupContext,
) -> io::Result<()> {
	let contents = fs::read_dir(source)?;

	for entry in contents {
		let entry = entry?;
		let path = entry.path();
		let file_name = entry.file_name();
		let dest_path = Path::new(dest).join(&file_name);
		let entry_rel_path = rel_path.join(&file_name);

		if path.is_symlink() {
			let target = fs::read_link(&path)?;
			#[cfg(unix)]
			symlink(&target, &dest_path)?;
			#[cfg(windows)]
			if target.is_dir() {
				symlink_dir(&target, &dest_path)?;
			} else {
				symlink_file(&target, &dest_path)?;
			}
		} else if path.is_dir() {
			fs::create_dir_all(&dest_path)?;

			// Recursively process subdirectories with the same previous backup path
			let prev_backup_subdir =
				prev_backup.map(|p| Path::new(p).join(&file_name).to_string_lossy().to_string());
			copy_folder(
				path.to_str().unwrap(),
				dest_path.to_str().unwrap(),
				prev_backup_subdir.as_deref(),
				&entry_rel_path,
				context,
			)?;
		} else {
			// Use the streaming copy implementation for regular files
			let prev_path = prev_backup.map(|p| Path::new(p).join(&file_name));

			// Output the full path of the file being processed
			println!("Processing: {}", path.display());

			copy_file_with_streaming(
				&path,
				&dest_path,
				prev_path.as_deref(),
				&entry_rel_path,
				context,
			)?;
		}
	}
	Ok(())
}
