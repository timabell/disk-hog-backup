use std::fs;
use std::io;
use std::path::Path;

use crate::backup_sets::backup_set;
use crate::backup_sets::set_namer;
use crate::dhcopy::copy_folder;
use crate::disk_space;

pub fn backup(source: &str, dest: &str) -> io::Result<String> {
	// Create the backup destination directory if it doesn't exist
	fs::create_dir_all(dest)?;

	// Get disk space at the start of backup
	let initial_disk_space = disk_space::get_disk_space(Path::new(dest))?;
	eprintln!("Target disk space before backup:");
	eprintln!(
		"  Total:     {} GB",
		initial_disk_space.total as f64 / 1_000_000_000.0
	);
	eprintln!(
		"  Available: {} GB",
		initial_disk_space.available as f64 / 1_000_000_000.0
	);
	eprintln!(
		"  Used:      {} GB",
		initial_disk_space.used as f64 / 1_000_000_000.0
	);
	eprintln!();

	// Find the most recent backup set to use for hardlinking
	let prev_backup = backup_set::find_most_recent_backup_set(dest);

	// Create a new backup set
	let backup_set_name = set_namer::generate_backup_set_name();
	let backup_set_path = Path::new(dest).join(&backup_set_name);
	fs::create_dir_all(&backup_set_path)?;

	eprintln!("Backing up {} into {:?} …", source, backup_set_path);
	if let Some(ref prev_backup) = prev_backup {
		eprintln!(
			"Found previous backup set to use for hard-linking: {}",
			prev_backup
		);
	}

	// Copy the source folder to the backup set, using hardlinks for unchanged files
	copy_folder::backup_folder(
		source,
		backup_set_path.to_str().unwrap(),
		prev_backup.as_deref(),
		&backup_set_name,
		Some(initial_disk_space),
	)?;

	// Get disk space after backup
	let final_disk_space = disk_space::get_disk_space(Path::new(dest))?;

	// Calculate and display the disk space usage summary
	let space_used = final_disk_space.used_difference(&initial_disk_space);

	eprintln!();
	eprintln!("=== Disk Space Summary ===");
	eprintln!("Before backup:");
	eprintln!(
		"  Available: {:.2} GB",
		initial_disk_space.available as f64 / 1_000_000_000.0
	);
	eprintln!(
		"  Used:      {:.2} GB",
		initial_disk_space.used as f64 / 1_000_000_000.0
	);
	eprintln!();
	eprintln!("After backup:");
	eprintln!(
		"  Available: {:.2} GB",
		final_disk_space.available as f64 / 1_000_000_000.0
	);
	eprintln!(
		"  Used:      {:.2} GB",
		final_disk_space.used as f64 / 1_000_000_000.0
	);
	eprintln!();
	eprintln!(
		"Additional space used: {:.2} GB",
		space_used.abs() as f64 / 1_000_000_000.0
	);
	eprintln!("==========================");

	Ok(backup_set_name)
}
