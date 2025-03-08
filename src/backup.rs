use crate::backup_sets::backup_set::{create_empty_set, find_most_recent_set};
use crate::dhcopy::copy_folder::copy_folder;
use chrono::Utc;
use std::fs;
use std::io;
use std::path::Path;

pub fn backup(source: &str, dest: &str) -> io::Result<String> {
	fs::create_dir_all(dest)?;

	// Find the most recent backup set, if any
	let prev_set = find_most_recent_set(dest);
	let prev_set_path = prev_set.as_ref().map(|set| Path::new(dest).join(set));

	// Create a new backup set
	let set_name = create_empty_set(dest, Utc::now)?;
	let dest_folder = Path::new(dest).join(&set_name);

	println!("Backing up {} into {:?} …", source, dest_folder);
	if let Some(prev_set) = prev_set {
		println!(
			"Found previous backup set to use for hard-linking: {}",
			prev_set
		);
	}

	// Pass the previous backup set path to the copy function
	copy_folder(
		source,
		dest_folder.to_str().unwrap(),
		prev_set_path.as_ref().map(|p| p.to_str().unwrap()),
	)?;

	println!("Backing completed of {} into {:?}", source, dest_folder);
	Ok(set_name)
}
