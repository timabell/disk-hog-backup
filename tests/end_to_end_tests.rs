use assert_cmd::Command;
use std::fs;
use std::io;
use std::path::Path;

mod common;
use common::{create_test_file, create_tmp_folder};

fn disk_hog_backup_cmd() -> Command {
	Command::cargo_bin("disk-hog-backup").expect("failed to find binary")
}

#[test]
fn test_backup_creates_new_files() -> io::Result<()> {
	// Set up source directory with a test file
	let source = create_tmp_folder("source")?;
	let test_file = "test.txt";
	let test_content = "Hello, backup!";
	create_test_file(&source, test_file, test_content)?;

	// Create backup destination
	let backup_root = create_tmp_folder("backups")?;

	// Run backup command with named arguments
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Verify backup contents
	let backup_sets = fs::read_dir(&backup_root)?;
	let backup_set = backup_sets
		.filter_map(Result::ok)
		.next()
		.expect("Should have created a backup set");

	let backed_up_file = backup_set.path().join(test_file);
	assert!(backed_up_file.exists(), "Backup file should exist");

	let original_content = fs::read_to_string(Path::new(&source).join(test_file))?;
	let backup_content = fs::read_to_string(&backed_up_file)?;
	assert_eq!(
		original_content, backup_content,
		"Backup file should have same contents"
	);

	Ok(())
}
