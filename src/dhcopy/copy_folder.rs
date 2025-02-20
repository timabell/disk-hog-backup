use std::fs;
use std::io;
use std::path::Path;

pub fn copy_folder(source: &str, dest: &str) -> io::Result<()> {
	println!("backing up folder {} into {}", source, dest);
	let contents = fs::read_dir(source)?;

	for entry in contents {
		let entry = entry?;
		let path = entry.path();
		let dest_path = Path::new(dest).join(entry.file_name());

		if path.is_symlink() {
			let target = fs::read_link(&path)?;
			std::os::unix::fs::symlink(target, &dest_path)?;
		} else if path.is_dir() {
			fs::create_dir_all(&dest_path)?;
			copy_folder(path.to_str().unwrap(), dest_path.to_str().unwrap())?;
		} else {
			fs::copy(&path, &dest_path)?;
		}
	}
	Ok(())
}
