use assert_cmd::Command;
use rand::Rng;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

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

#[test]
fn test_backup_nested_files() -> Result<(), Box<dyn std::error::Error>> {
	// Create source folder with nested structure
	let source = create_tmp_folder("orig")?;
	let nested_folder = Path::new(&source).join("folder1/folder2/folder3");
	fs::create_dir_all(&nested_folder)?;

	// Create a nested test file
	let nested_file = nested_folder.join("nested.txt");
	fs::write(&nested_file, "nested content")?;

	// Create backup folder
	let dest = create_tmp_folder("backups")?;

	// Run the backup
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&dest)
		.assert()
		.success();

	// Find the backup set folder
	let backup_sets: Vec<_> = fs::read_dir(&dest)?.collect();
	assert_eq!(backup_sets.len(), 1, "should create exactly one backup set");
	let backup_set = backup_sets[0].as_ref().unwrap();

	// Check nested file was backed up with folder structure
	let nested_backup = backup_set.path().join("folder1/folder2/folder3/nested.txt");
	assert!(nested_backup.exists(), "nested file should be backed up");
	assert_eq!(fs::read_to_string(&nested_backup)?, "nested content");

	// Cleanup
	fs::remove_dir_all(&source)?;
	fs::remove_dir_all(&dest)?;

	Ok(())
}

#[test]
fn test_backup_empty_nested_folders() -> Result<(), Box<dyn std::error::Error>> {
	// Create source folder with nested empty folders
	let source = create_tmp_folder("orig")?;
	let nested_empty_folder = Path::new(&source).join("empty1/empty2/empty3");
	fs::create_dir_all(&nested_empty_folder)?;

	// Create backup folder
	let dest = create_tmp_folder("backups")?;

	// Run the backup
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&dest)
		.assert()
		.success();

	// Find the backup set folder
	let backup_sets: Vec<_> = fs::read_dir(&dest)?.collect();
	assert_eq!(backup_sets.len(), 1, "should create exactly one backup set");
	let backup_set = backup_sets[0].as_ref().unwrap();

	// Check empty nested folders were backed up
	let nested_backup = backup_set.path().join("empty1/empty2/empty3");
	assert!(
		nested_backup.exists(),
		"empty nested folders should be backed up"
	);
	assert!(nested_backup.is_dir(), "should be a directory");

	// Verify the folder is empty
	let dir_contents: Vec<_> = fs::read_dir(&nested_backup)?.collect();
	assert_eq!(dir_contents.len(), 0, "folder should be empty");

	// Cleanup
	fs::remove_dir_all(&source)?;
	fs::remove_dir_all(&dest)?;

	Ok(())
}

fn disk_hog_backup_cmd() -> Command {
	Command::cargo_bin("disk-hog-backup").expect("failed to find binary")
}

pub fn create_tmp_folder(prefix: &str) -> io::Result<String> {
	let mut rng = rand::rng();
	let random_suffix: u32 = rng.random();
	let dir = env::temp_dir().join(format!("dhb-{}-{}", prefix, random_suffix));
	fs::create_dir_all(&dir)?;
	Ok(dir.to_string_lossy().into_owned())
}

pub fn create_test_file(folder: &str, filename: &str, contents: &str) -> io::Result<()> {
	let file_path = Path::new(folder).join(filename);
	let mut file = fs::File::create(file_path)?;
	io::Write::write_all(&mut file, contents.as_bytes())?;
	Ok(())
}
