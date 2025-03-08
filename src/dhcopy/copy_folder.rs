use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::fs::hard_link as link;
#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

use crate::dhcopy::file_hash;

pub fn copy_folder(source: &str, dest: &str, prev_backup: Option<&str>) -> io::Result<()> {
	println!("backing up folder {} into {}", source, dest);
	let contents = fs::read_dir(source)?;

	for entry in contents {
		let entry = entry?;
		let path = entry.path();
		let rel_path = entry.file_name();
		let dest_path = Path::new(dest).join(&rel_path);

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
				prev_backup.map(|p| Path::new(p).join(&rel_path).to_string_lossy().to_string());
			copy_folder(
				path.to_str().unwrap(),
				dest_path.to_str().unwrap(),
				prev_backup_subdir.as_deref(),
			)?;
		} else {
			// Check if the file exists in the previous backup
			if let Some(prev) = prev_backup {
				let prev_path = Path::new(prev).join(&rel_path);

				if prev_path.exists() && !prev_path.is_dir() {
					// Check if the file content is the same using MD5
					match file_hash::files_match(&path, &prev_path) {
						Ok(true) => {
							// Files match, create a hardlink
							#[cfg(unix)]
							{
								println!("Hardlinking {} (unchanged)", rel_path.to_string_lossy());
								link(&prev_path, &dest_path)?;
								continue; // Skip the copy below
							}

							// On Windows or other platforms, fall back to copying
							#[cfg(not(unix))]
							{
								println!(
									"File unchanged: {} (copying, hardlinks not supported)",
									rel_path.to_string_lossy()
								);
							}
						}
						Ok(false) => {
							println!("File changed: {}", rel_path.to_string_lossy());
						}
						Err(e) => {
							println!(
								"Error comparing files: {} - {}",
								rel_path.to_string_lossy(),
								e
							);
						}
					}
				}
			}

			// If we got here, either there's no previous backup, the file doesn't exist in it,
			// the files don't match, or we're on a platform that doesn't support hardlinks
			fs::copy(&path, &dest_path)?;
		}
	}
	Ok(())
}
