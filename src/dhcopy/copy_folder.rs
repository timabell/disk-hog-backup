use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::fs::hard_link as link;
#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

use crate::dhcopy::streaming_copy::copy_file_with_streaming;

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
			// Use the streaming copy implementation for regular files
			let prev_path = prev_backup.map(|p| Path::new(p).join(&rel_path));
			copy_file_with_streaming(&path, &dest_path, prev_path.as_deref())?;
		}
	}
	Ok(())
}
