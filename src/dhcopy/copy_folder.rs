use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

pub fn copy_folder(source: &str, dest: &str) -> io::Result<()> {
	println!("backing up folder {} into {}", source, dest);
	let contents = fs::read_dir(source)?;

	for entry in contents {
		let entry = entry?;
		let path = entry.path();
		let dest_path = Path::new(dest).join(entry.file_name());

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
			copy_folder(path.to_str().unwrap(), dest_path.to_str().unwrap())?;
		} else {
			fs::copy(&path, &dest_path)?;
		}
	}
	Ok(())
}
