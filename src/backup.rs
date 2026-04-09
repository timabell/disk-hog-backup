use std::fs;
use std::io;
use std::path::Path;

use crate::backup_sets::WIP_PREFIX;
use crate::backup_sets::backup_set;
use crate::backup_sets::set_namer;
use crate::dhcopy::copy_folder;
use crate::disk_space;
use bytesize::ByteSize;

/// Performs a backup from source to destination.
///
/// Creates backup sets with a `wip_` prefix during backup, then renames to the
/// final name upon successful completion. This ensures that incomplete backups
/// (e.g. process killed, out of disk space) are not mistaken for complete sets,
/// which could be a data integrity disaster.
pub fn backup(source: &str, dest: &str, auto_delete: bool) -> io::Result<String> {
	// Create the backup destination directory if it doesn't exist
	fs::create_dir_all(dest)?;

	// Get disk space at the start of backup
	let initial_disk_space = disk_space::get_disk_space(Path::new(dest))?;
	eprintln!("Target disk space before backup:");
	eprintln!("  Total:     {}", ByteSize(initial_disk_space.total));
	eprintln!("  Available: {}", ByteSize(initial_disk_space.available));
	eprintln!("  Used:      {}", ByteSize(initial_disk_space.used));
	eprintln!();

	// Find the most recent backup set to use for hardlinking
	let prev_backup = backup_set::find_most_recent_backup_set(dest);

	// Create a new backup set with wip_ prefix (renamed on completion)
	let backup_set_name = set_namer::generate_backup_set_name();
	let wip_backup_set_name = format!("{}{}", WIP_PREFIX, backup_set_name);
	let wip_backup_set_path = Path::new(dest).join(&wip_backup_set_name);
	fs::create_dir_all(&wip_backup_set_path)?;

	eprintln!("Backing up {} into {:?} …", source, wip_backup_set_path);
	if let Some(ref prev_backup) = prev_backup {
		eprintln!(
			"Found previous backup set to use for hard-linking: {}",
			prev_backup
		);
	}

	// Copy the source folder to the backup set, using hardlinks for unchanged files.
	// Pass the final name (without wip_) as session_id so MD5/stats files use the final name.
	copy_folder::backup_folder(
		source,
		wip_backup_set_path.to_str().unwrap(),
		prev_backup.as_deref(),
		&backup_set_name,
		Some(initial_disk_space),
		copy_folder::AutoDeleteConfig {
			enabled: auto_delete,
			backup_root: dest,
		},
	)?;

	// Backup completed successfully - rename backup set folder from wip_ to final name
	// (MD5 and stats files already use the final name via session_id)
	let final_backup_set_path = Path::new(dest).join(&backup_set_name);
	fs::rename(&wip_backup_set_path, &final_backup_set_path)?;

	Ok(backup_set_name)
}
