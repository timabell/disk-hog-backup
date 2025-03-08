use assert_cmd::Command;
use chrono;
use rand::Rng;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

#[test]
fn test_backup_of_single_text_file() -> io::Result<()> {
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
fn test_backup_nested_file() -> Result<(), Box<dyn std::error::Error>> {
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

	Ok(())
}

#[test]
fn test_backup_empty_nested_folder() -> Result<(), Box<dyn std::error::Error>> {
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

	Ok(())
}

#[test]
fn test_backup_set_naming() -> Result<(), Box<dyn std::error::Error>> {
	// Create test folders
	let source = create_tmp_folder("orig")?;
	let dest = create_tmp_folder("backups")?;

	// Record time just before backup
	let before_backup = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0)
		.unwrap()
		.naive_utc();

	// Run backup and capture output
	let output = disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&dest)
		.output()?;

	// Record time just after backup
	let after_backup = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0)
		.unwrap()
		.naive_utc();

	assert!(output.status.success(), "backup command should succeed");

	// Extract set name from output message
	let stdout = String::from_utf8(output.stdout)?;
	let set_name = stdout
		.lines()
		.find(|line| line.contains("backing up") && line.contains("into"))
		.and_then(|line| {
			// Extract the path after "into"
			line.split("into").nth(1)
		})
		.and_then(|path| {
			// Extract last path component
			Path::new(path.trim())
				.file_name()
				.and_then(|s| s.to_str())
				.map(|s| s.trim_matches('"'))
		})
		.ok_or("Could not find set name in output")?;

	// Parse the timestamp from set name (format: dhb-set-YYYYMMDD-HHMMSS)
	assert!(
		set_name.starts_with("dhb-set-"),
		"set name should start with dhb-set-"
	);
	let datetime_str = &set_name[8..]; // Skip "dhb-set-"
	let backup_time = chrono::NaiveDateTime::parse_from_str(datetime_str, "%Y%m%d-%H%M%S")?;

	// Verify timestamp is within the execution window (allowing for second rollover)
	assert!(
		backup_time >= before_backup && backup_time <= after_backup,
		"backup timestamp {} should be between {} and {}",
		backup_time,
		before_backup,
		after_backup
	);

	// Verify the folder exists on disk with exactly this name
	let backup_folder = Path::new(&dest).join(set_name);
	assert!(
		backup_folder.exists(),
		"backup folder {} should exist on disk",
		backup_folder.display()
	);

	Ok(())
}

#[cfg(unix)]
#[test]
fn test_backup_with_symlinks() -> io::Result<()> {
	// Set up source directory with a test file and symlinks
	let source = create_tmp_folder("source")?;

	// Create a real file
	let test_file = "test.txt";
	let test_content = "Hello, backup!";
	create_test_file(&source, test_file, test_content)?;

	// Create a symlink to the file
	let symlink_path = Path::new(&source).join("link_to_test.txt");
	std::os::unix::fs::symlink(Path::new(&source).join(test_file), &symlink_path)?;

	// Create a symlink to a directory
	let target_dir = Path::new(&source).join("target_dir");
	fs::create_dir(&target_dir)?;
	create_test_file(
		&target_dir.to_str().unwrap(),
		"target.txt",
		"target content",
	)?;

	let dir_symlink_path = Path::new(&source).join("link_to_dir");
	std::os::unix::fs::symlink(&target_dir, &dir_symlink_path)?;

	// Create backup destination
	let backup_root = create_tmp_folder("backups")?;

	// Run backup command
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Get the backup set directory
	let backup_sets = fs::read_dir(&backup_root)?;
	let backup_set = backup_sets
		.filter_map(Result::ok)
		.next()
		.expect("Should have created a backup set");

	// Verify file symlink
	let backed_up_symlink = backup_set.path().join("link_to_test.txt");
	assert!(backed_up_symlink.exists(), "Backup symlink should exist");
	assert!(
		backed_up_symlink.is_symlink(),
		"Should be a symlink in backup"
	);

	let symlink_target = fs::read_link(&backed_up_symlink)?;
	assert_eq!(
		symlink_target,
		Path::new(&source).join(test_file),
		"Symlink should point to original target"
	);

	// Verify directory symlink
	let backed_up_dir_symlink = backup_set.path().join("link_to_dir");
	assert!(
		backed_up_dir_symlink.exists(),
		"Backup dir symlink should exist"
	);
	assert!(
		backed_up_dir_symlink.is_symlink(),
		"Should be a symlink in backup"
	);

	let dir_symlink_target = fs::read_link(&backed_up_dir_symlink)?;
	assert_eq!(
		dir_symlink_target, target_dir,
		"Directory symlink should point to original target"
	);

	Ok(())
}

#[cfg(windows)]
#[test]
fn test_backup_with_windows_symlinks() -> io::Result<()> {
	// Set up source directory with a test file and symlinks
	let source = create_tmp_folder("source")?;

	// Create a real file
	let test_file = "test.txt";
	let test_content = "Hello, backup!";
	create_test_file(&source, test_file, test_content)?;

	// Create a symlink to the file
	let symlink_path = Path::new(&source).join("link_to_test.txt");
	std::os::windows::fs::symlink_file(Path::new(&source).join(test_file), &symlink_path)?;

	// Create a symlink to a directory
	let target_dir = Path::new(&source).join("target_dir");
	fs::create_dir(&target_dir)?;
	create_test_file(
		&target_dir.to_str().unwrap(),
		"target.txt",
		"target content",
	)?;

	let dir_symlink_path = Path::new(&source).join("link_to_dir");
	std::os::windows::fs::symlink_dir(&target_dir, &dir_symlink_path)?;

	// Create backup destination
	let backup_root = create_tmp_folder("backups")?;

	// Run backup command
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Get the backup set directory
	let backup_sets = fs::read_dir(&backup_root)?;
	let backup_set = backup_sets
		.filter_map(Result::ok)
		.next()
		.expect("Should have created a backup set");

	// Verify file symlink
	let backed_up_symlink = backup_set.path().join("link_to_test.txt");
	assert!(backed_up_symlink.exists(), "Backup symlink should exist");
	assert!(
		backed_up_symlink.is_symlink(),
		"Should be a symlink in backup"
	);

	let symlink_target = fs::read_link(&backed_up_symlink)?;
	assert_eq!(
		symlink_target,
		Path::new(&source).join(test_file),
		"Symlink should point to original target"
	);

	// Verify directory symlink
	let backed_up_dir_symlink = backup_set.path().join("link_to_dir");
	assert!(
		backed_up_dir_symlink.exists(),
		"Backup dir symlink should exist"
	);
	assert!(
		backed_up_dir_symlink.is_symlink(),
		"Should be a symlink in backup"
	);

	let dir_symlink_target = fs::read_link(&backed_up_dir_symlink)?;
	assert_eq!(
		dir_symlink_target, target_dir,
		"Directory symlink should point to original target"
	);

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
	println!("Created temp folder: {}", dir.to_string_lossy());
	Ok(dir.to_string_lossy().into_owned())
}

pub fn create_test_file(folder: &str, filename: &str, contents: &str) -> io::Result<()> {
	let file_path = Path::new(folder).join(filename);
	let mut file = fs::File::create(file_path)?;
	io::Write::write_all(&mut file, contents.as_bytes())?;
	Ok(())
}
