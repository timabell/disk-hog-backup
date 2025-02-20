use crate::backup_sets::backup_set::create_empty_set;
use crate::dhcopy::copy_folder::copy_folder;
use chrono::Utc;
use std::fs;
use std::io;
use std::path::Path;

pub fn backup(source: &str, dest: &str) -> io::Result<String> {
	fs::create_dir_all(dest)?;
	let set_name = create_empty_set(dest, || Utc::now())?;
	let dest_folder = Path::new(dest).join(&set_name);
	println!("backing up {} into {:?}", source, dest_folder);
	copy_folder(source, dest_folder.to_str().unwrap())?;
	Ok(set_name)
}
