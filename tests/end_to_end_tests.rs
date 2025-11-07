use assert_cmd::Command;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time;

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
fn test_metadata_preserved() -> io::Result<()> {
	// Set up source directory with a test file
	let source = create_tmp_folder("source_meta")?;
	let test_file = "metadata_test.txt";
	let test_content = "This file has custom metadata!";

	// Create the test file
	let source_file_path = Path::new(&source).join(test_file);
	fs::write(&source_file_path, test_content)?;

	// Set a specific modification time (30 seconds in the past)
	let now = time::SystemTime::now();
	let thirty_seconds_ago = now - time::Duration::from_secs(30);

	// Set the file's modification time
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;

		// Set file permissions to read-only for owner (0o400)
		let mut perms = fs::metadata(&source_file_path)?.permissions();
		perms.set_mode(0o400);
		fs::set_permissions(&source_file_path, perms)?;
	}

	// Set modification time on all platforms
	filetime::set_file_mtime(
		&source_file_path,
		filetime::FileTime::from_system_time(thirty_seconds_ago),
	)?;

	// Get original metadata for later comparison
	let original_metadata = fs::metadata(&source_file_path)?;
	let original_mtime = filetime::FileTime::from_last_modification_time(&original_metadata);

	// Create backup destination
	let backup_root = create_tmp_folder("backups_meta")?;

	// Run backup command
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Find the backup file
	let backup_sets = fs::read_dir(&backup_root)?;
	let backup_set = backup_sets
		.filter_map(Result::ok)
		.next()
		.expect("Should have created a backup set");

	let backed_up_file = backup_set.path().join(test_file);
	assert!(backed_up_file.exists(), "Backup file should exist");

	// Compare metadata
	let backup_metadata = fs::metadata(&backed_up_file)?;
	let backup_mtime = filetime::FileTime::from_last_modification_time(&backup_metadata);

	// Verify modification time is preserved
	assert_eq!(
		original_mtime, backup_mtime,
		"Backup file should preserve modification time"
	);

	// Verify file permissions are preserved (Unix only)
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;

		let original_mode = original_metadata.permissions().mode() & 0o777; // Only compare permission bits
		let backup_mode = backup_metadata.permissions().mode() & 0o777;

		assert_eq!(
			original_mode, backup_mode,
			"Backup file should preserve file permissions"
		);
	}

	// Verify file size is preserved
	assert_eq!(
		original_metadata.len(),
		backup_metadata.len(),
		"Backup file should preserve file size"
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

	// Extract set name from stderr (all output goes to stderr for progress display)
	let stderr = String::from_utf8(output.stderr)?;
	let set_name = stderr
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
	create_test_file(target_dir.to_str().unwrap(), "target.txt", "target content")?;

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

#[cfg(unix)]
#[test]
fn test_hardlinking_in_second_backup_set() -> Result<(), Box<dyn std::error::Error>> {
	// Set up source directory with two test files
	let source = create_tmp_folder("source")?;

	// Create two test files
	let file1 = "file1.txt";
	let file2 = "file2.txt";
	let content1 = "This is file 1 content";
	let content2 = "This is file 2 content";
	create_test_file(&source, file1, content1)?;
	create_test_file(&source, file2, content2)?;

	// Create backup destination
	let backup_root = create_tmp_folder("backups")?;

	// Run first backup
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Find the first backup set
	let backup_sets: Vec<_> = fs::read_dir(&backup_root)?.filter_map(Result::ok).collect();
	assert_eq!(backup_sets.len(), 1, "Should create exactly one backup set");
	let first_backup_set = &backup_sets[0];

	// Verify first backup files exist
	let first_backup_file1 = first_backup_set.path().join(file1);
	let first_backup_file2 = first_backup_set.path().join(file2);
	assert!(
		first_backup_file1.exists(),
		"First backup file1 should exist"
	);
	assert!(
		first_backup_file2.exists(),
		"First backup file2 should exist"
	);

	// Modify file1 in source
	let modified_content = "This is file 1 with modified content";
	create_test_file(&source, file1, modified_content)?;

	// Add a delay to ensure different timestamps for backup sets
	thread::sleep(time::Duration::from_secs(2));

	// Run second backup
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Find the second backup set (should be the newest one)
	let backup_sets: Vec<_> = fs::read_dir(&backup_root)?.filter_map(Result::ok).collect();
	assert_eq!(backup_sets.len(), 2, "Should now have two backup sets");

	// Sort backup sets by creation time to find the newest one
	let mut backup_sets = backup_sets;
	backup_sets.sort_by_key(|entry| entry.path().metadata().unwrap().created().unwrap());
	let second_backup_set = &backup_sets[1];

	// Get paths to the second backup files
	let second_backup_file1 = second_backup_set.path().join(file1);
	let second_backup_file2 = second_backup_set.path().join(file2);

	// Verify both files exist in second backup
	assert!(
		second_backup_file1.exists(),
		"Second backup file1 should exist"
	);
	assert!(
		second_backup_file2.exists(),
		"Second backup file2 should exist"
	);

	// Verify file1 content was updated
	let second_backup_file1_content = fs::read_to_string(&second_backup_file1)?;
	assert_eq!(
		second_backup_file1_content, modified_content,
		"File1 in second backup should have modified content"
	);

	// Verify file2 content is unchanged
	let second_backup_file2_content = fs::read_to_string(&second_backup_file2)?;
	assert_eq!(
		second_backup_file2_content, content2,
		"File2 in second backup should have original content"
	);

	// Check if file2 is hardlinked (same inode) between backups
	let first_inode = get_inode(&first_backup_file2)?;
	let second_inode = get_inode(&second_backup_file2)?;
	assert_eq!(
		first_inode, second_inode,
		"Unmodified file2 should be hardlinked (same inode) between backup sets"
	);

	// Check that file1 is NOT hardlinked (different inodes) between backups
	let first_inode = get_inode(&first_backup_file1)?;
	let second_inode = get_inode(&second_backup_file1)?;
	assert_ne!(
		first_inode, second_inode,
		"Modified file1 should NOT be hardlinked (different inodes) between backup sets"
	);

	Ok(())
}

#[cfg(unix)]
fn get_inode(path: &Path) -> io::Result<u64> {
	use std::os::unix::fs::MetadataExt;
	let metadata = fs::metadata(path)?;
	Ok(metadata.ino())
}

#[test]
fn test_dhbignore_functionality() -> io::Result<()> {
	// Arrange
	let source = create_tmp_folder("source_ignore")?;
	let destination = create_tmp_folder("backups_ignore")?;

	create_test_file(
		&source,
		".dhbignore",
		r#"
# This comment should be ignored without error
# Format roughly based on https://git-scm.com/docs/gitignore

# wildcards
*.tmp
*temp*
bad*news
nope-*

# match exact file or folder at any depth
evil

# match only folders when trailing slash present, at any depth
build/
snowflake/
bing*bong/

# absolute paths - only match at root level
alpha/beta
alpha/delta/
/gamma
/root-file.txt
/nested/absolute/specific-file.txt

# negation: ignore log and then exclude one file from that
*.log
!important.log

"#,
	)?;

	let ignored_files = vec![
		"ignore-me.tmp",
		"nested/ignore-me.tmp",
		"nope-1234",
		"nope-456/bar.txt",
		"evil",
		"nested/foo/evil",
		"nested/folder/snowflake/ice.txt",
		"nested/evil/monkey.txt",
		"build/output.txt",
		"nested/build/output.txt",
		"nested/really/deeply/build/output.txt",
		"bing-bang-bong/something.txt",
		"alpha/beta/foo.txt",
		"alpha/delta/foo.txt",
		"gamma/foo.txt",
		"root-file.txt",
		"nested/absolute/specific-file.txt",
		"unimportant.log",
	];
	for path in &ignored_files {
		create_test_file(&source, path, "ignored file")?;
	}

	let kept_files = vec![
		"dont-ignore-me.txt",
		"nested/not-evil/monkey.txt",
		"nested/inner/build",
		"nested/alpha/beta/foo.txt",
		"nested/alpha/delta/foo.txt",
		"nested/gamma/foo.txt",
		"nested/really/deeply/foo.txt",
		"nested/file/snowflake",
		"nested/root-file.txt",
		"specific-file.txt",
		"nested/specific-file.txt",
		"not-root/nested/absolute/specific-file.txt",
		"important.log",
	];
	for path in &kept_files {
		create_test_file(&source, path, "kept file")?;
	}

	// Act
	// Run backup command
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&destination)
		.assert()
		.success();

	// Assert
	// Find the backup set
	let backup_sets = fs::read_dir(&destination)?;
	let backup_set = backup_sets
		.filter_map(Result::ok)
		.next()
		.expect("Should have created a backup set");

	for path in ignored_files {
		assert!(
			!Path::new(&backup_set.path()).join(path).exists(),
			"'{}' should be ignored",
			path
		);
	}
	for path in kept_files {
		assert!(
			Path::new(&backup_set.path()).join(path).exists(),
			"'{}' should be kept",
			path
		);
	}

	Ok(())
}

#[test]
fn test_backup_of_hidden_file() -> io::Result<()> {
	// Set up source directory with a hidden file
	let source = create_tmp_folder("source_hidden")?;

	// Create a hidden file in the source directory
	fs::write(
		Path::new(&source).join(".hidden_file"),
		"hidden file content",
	)?;

	// Create backup destination
	let backup_root = create_tmp_folder("backups_hidden")?;

	// Run backup command
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Find the backup set
	let backup_sets = fs::read_dir(&backup_root)?;
	let backup_set = backup_sets
		.filter_map(Result::ok)
		.next()
		.expect("Should have created a backup set");

	// Check that the hidden file was backed up
	let hidden_file_path = Path::new(&backup_set.path()).join(".hidden_file");
	assert!(hidden_file_path.exists(), "Hidden file should be backed up");

	// Check the content of the hidden file
	let content = fs::read_to_string(hidden_file_path)?;
	assert_eq!(
		content, "hidden file content",
		"Hidden file content should match"
	);

	Ok(())
}

#[cfg(unix)]
#[test]
fn test_backup_skips_special_files() -> io::Result<()> {
	use std::process::Command as StdCommand;

	// Set up source directory
	let source = create_tmp_folder("source_special")?;

	// Create a regular file
	let regular_file = "regular.txt";
	let regular_content = "This is a regular file";
	create_test_file(&source, regular_file, regular_content)?;

	// Create a named pipe (FIFO)
	let fifo_path = Path::new(&source).join("test_fifo");
	let fifo_path_str = fifo_path.to_str().unwrap();

	// Use mkfifo to create the named pipe
	StdCommand::new("mkfifo")
		.arg(fifo_path_str)
		.status()
		.expect("Failed to create named pipe");

	assert!(fifo_path.exists(), "Named pipe should exist");

	// Create backup destination
	let backup_root = create_tmp_folder("backups_special")?;

	// Run backup command
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

	// Regular file should be backed up
	let backed_up_regular = backup_set.path().join(regular_file);
	assert!(
		backed_up_regular.exists(),
		"Regular file should be backed up"
	);

	// FIFO should NOT be backed up
	let backed_up_fifo = backup_set.path().join("test_fifo");
	assert!(!backed_up_fifo.exists(), "FIFO should not be backed up");

	Ok(())
}

#[cfg(not(unix))]
#[test]
fn test_backup_skips_special_files() -> io::Result<()> {
	// This test is Unix-specific, so just pass on non-Unix platforms
	Ok(())
}

fn disk_hog_backup_cmd() -> Command {
	Command::cargo_bin("disk-hog-backup").expect("failed to find binary")
}

pub fn create_tmp_folder(prefix: &str) -> io::Result<String> {
	let random_suffix: u32 = rand::random();
	let dir = env::temp_dir().join(format!("dhb-{}-{}", prefix, random_suffix));
	fs::create_dir_all(&dir)?;
	println!("Created temp folder: {}", dir.to_string_lossy());
	Ok(dir.to_string_lossy().into_owned())
}

pub fn create_test_file(base_folder: &str, path: &str, contents: &str) -> io::Result<()> {
	// create folder if missing:
	let path = Path::new(base_folder).join(path);
	fs::create_dir_all(path.parent().unwrap())?;
	// create file
	let mut file = fs::File::create(path)?;
	// write contents
	io::Write::write_all(&mut file, contents.as_bytes())?;
	Ok(())
}

#[test]
fn test_backup_stats_functionality() -> io::Result<()> {
	use regex::Regex;

	// Set up source directory with a simple test file
	let source = create_tmp_folder("stats_source")?;
	create_test_file(&source, "test.txt", "test content")?;

	// Create backup destination
	let backup_root = create_tmp_folder("stats_backups")?;

	// Run backup
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Get the backup set directory
	let backup_sets: Vec<_> = fs::read_dir(&backup_root)?.filter_map(Result::ok).collect();
	let backup_set_path = backup_sets[0].path();
	let stats_file = backup_set_path.join("disk-hog-backup-stats.txt");
	let stats_content = fs::read_to_string(&stats_file)?;

	// Normalize timestamps and durations using regex
	let normalized = stats_content.clone();

	// Normalize version
	let normalized = Regex::new(r"disk-hog-backup \d+\.\d+\.\d+(?:-git)?")
		.unwrap()
		.replace_all(&normalized, "disk-hog-backup VERSION");

	// Normalize timestamps
	let normalized = Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} UTC")
		.unwrap()
		.replace_all(&normalized, "YYYY-MM-DD HH:MM:SS.mmm UTC");

	// Normalize durations
	let normalized = Regex::new(r"\d{2}:\d{2}:\d{2}\.\d{3}")
		.unwrap()
		.replace_all(&normalized, "HH:MM:SS.mmm");

	// Normalize session IDs
	let normalized = Regex::new(r"dhb-set-\d{8}-\d{6}")
		.unwrap()
		.replace_all(&normalized, "dhb-set-YYYYMMDD-HHMMSS");

	// Normalize pipeline performance times (seconds with percentages)
	let normalized = Regex::new(r"\d+\.\d+s \(\s*\d+\.\d+%\)")
		.unwrap()
		.replace_all(&normalized, "X.XXs ( XX.X%)");

	// Normalize percentages (including 0%)
	let normalized = Regex::new(r"\d+(?:\.\d+)?%")
		.unwrap()
		.replace_all(&normalized, "XX.X%");

	// Normalize queue stats numbers
	let normalized = Regex::new(r"Avg: \d+\.\d+")
		.unwrap()
		.replace_all(&normalized, "Avg: X.X");
	let normalized = Regex::new(r"Peak: \d+")
		.unwrap()
		.replace_all(&normalized, "Peak: X");

	// Normalize disk space numbers only in Disk Space section lines
	// Match patterns like "123.4 GB", "45 MB", "6.7 KB", "89 B" or binary units like "123.4 GiB", "45 MiB", "6.7 KiB"
	let normalized =
		Regex::new(r"(\d+(?:\.\d+)?) ((?:GiB|MiB|KiB|GB|MB|KB|B)) (used|total|available|additional space)")
			.unwrap()
			.replace_all(&normalized, "X.X XB $3");
	// Also normalize the MD5 store line which ends with just the size
	let normalized = Regex::new(r"(MD5 store:\s+)\d+(?:\.\d+)? (?:GiB|MiB|KiB|GB|MB|KB|B)")
		.unwrap()
		.replace_all(&normalized, "${1}X.X XB");

	// Strip trailing whitespace and bar chart characters from each line
	let mut normalized = normalized
		.lines()
		.map(|line| {
			// Remove bar chart characters and trailing whitespace
			Regex::new(r"[█▓▒░\s]+$")
				.unwrap()
				.replace(line, "")
				.to_string()
		})
		.collect::<Vec<_>>()
		.join("\n");

	// Add trailing newline to match file format
	normalized.push('\n');

	let expected_stats = r"Backup Summary
==============
Program: disk-hog-backup VERSION
Time format: HH:MM:SS.mmm
Sizes: bytes (with human-readable shown)

Session ID: dhb-set-YYYYMMDD-HHMMSS

Time:
  Started:  YYYY-MM-DD HH:MM:SS.mmm UTC
  Size Calc: HH:MM:SS.mmm
  Finished: YYYY-MM-DD HH:MM:SS.mmm UTC
  Duration: HH:MM:SS.mmm

Backup Set Stats:
  New:              1
  Size changed:     0
  Mtime changed:    0
  Content changed:  0
  Hardlinked:       0 files, 0 B
  Copied:           1 files, 12 B
  Total:            1 files, 12 B

I/O:
  Source Read: 12 (12 B)
  Target Read: 0 (0 B)
  Target Written: 12 (12 B)
  Hashing: 12 (12 B)

Pipeline Performance:

Reader Thread:
  I/O                   X.XXs ( XX.X%)
  Send->Writer          X.XXs ( XX.X%)
  Send->Hasher          X.XXs ( XX.X%)
  Throttle              X.XXs ( XX.X%)

Hasher Thread:
  Blocked (recv)        X.XXs ( XX.X%)
  Hash (MD5)            X.XXs ( XX.X%)

Writer Thread:
  Blocked (recv)        X.XXs ( XX.X%)
  I/O                   X.XXs ( XX.X%)

Pipeline:
  Reader I/O: XX.X%, Hasher: XX.X%, Writer I/O: XX.X%
  Assessment: Pipeline appears well-tuned

Queue Stats:
  Writer Queue: Avg: X.X/32 (XX.X%) | Peak: X/32
  Hasher Queue: Avg: X.X/32 (XX.X%) | Peak: X/32


Disk Space:
  Initial:    X.X XB used of X.X XB total (X.X XB available)
  Final:      X.X XB used of X.X XB total (X.X XB available)
  Backup used: X.X XB additional space
  MD5 store:   X.X XB
";

	assert_eq!(
		&normalized, expected_stats,
		"Stats file should match exactly"
	);

	Ok(())
}

#[test]
fn test_backup_with_directory_symlink_loop() -> io::Result<()> {
	// This test verifies that the backup doesn't infinite loop when
	// encountering directory symlinks that create cycles

	let source = create_tmp_folder("symlink_loop_source")?;

	// Create a real file in the root
	create_test_file(&source, "root_file.txt", "root content")?;

	// Create a subdirectory with a file
	let subdir = Path::new(&source).join("subdir");
	fs::create_dir(&subdir)?;
	create_test_file(
		subdir.to_str().unwrap(),
		"subdir_file.txt",
		"subdir content",
	)?;

	// Create a symlink in subdir that points back to the parent directory
	// This creates a cycle: source -> subdir -> link_to_parent -> source
	let symlink_to_parent = subdir.join("link_to_parent");
	std::os::unix::fs::symlink(&source, &symlink_to_parent)?;

	// Create backup destination
	let backup_root = create_tmp_folder("symlink_loop_backups")?;

	// Run backup command - should complete without infinite loop
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Get the backup set directory
	let backup_sets: Vec<_> = fs::read_dir(&backup_root)?.filter_map(Result::ok).collect();
	assert_eq!(backup_sets.len(), 1, "Should have exactly one backup set");
	let backup_set = &backup_sets[0];

	// Verify the real files were backed up
	let backed_up_root_file = backup_set.path().join("root_file.txt");
	assert!(
		backed_up_root_file.exists(),
		"Root file should be backed up"
	);
	assert_eq!(
		fs::read_to_string(&backed_up_root_file)?,
		"root content",
		"Root file content should match"
	);

	let backed_up_subdir_file = backup_set.path().join("subdir/subdir_file.txt");
	assert!(
		backed_up_subdir_file.exists(),
		"Subdir file should be backed up"
	);
	assert_eq!(
		fs::read_to_string(&backed_up_subdir_file)?,
		"subdir content",
		"Subdir file content should match"
	);

	// Verify the symlink was created as a symlink (not followed)
	let backed_up_symlink = backup_set.path().join("subdir/link_to_parent");
	assert!(backed_up_symlink.exists(), "Symlink should exist in backup");
	assert!(
		backed_up_symlink.is_symlink(),
		"Should be a symlink, not a directory"
	);

	println!("✓ Directory symlink loop test passed!");
	println!("  Symlink was preserved, not followed");
	println!("  No infinite loop occurred");

	Ok(())
}

#[test]
fn test_hardlinking_optimization() -> Result<(), Box<dyn std::error::Error>> {
	// Set up source directory with test files
	let source = create_tmp_folder("hardlink_opt_source")?;

	create_test_file(&source, "unchanged.txt", "unchanged")?;
	create_test_file(&source, "size_change.txt", "original")?;
	create_test_file(&source, "mtime_change.txt", "content")?;
	create_test_file(&source, "content_change.txt", "original")?;

	// Create backup destination
	let backup_root = create_tmp_folder("hardlink_opt_backups")?;

	// Run first backup
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	thread::sleep(time::Duration::from_secs(2));

	// Modify files in different ways
	// unchanged.txt - leave alone (should be trusted via mtime)
	// size_change.txt - change size (should increment size_changed)
	create_test_file(&source, "size_change.txt", "different length content")?;
	// mtime_change.txt - touch only (should increment mtime_changed, but hardlink)
	let mtime_path = Path::new(&source).join("mtime_change.txt");
	let content = fs::read_to_string(&mtime_path)?;
	fs::write(&mtime_path, content)?;
	// content_change.txt - change content same length (should increment mtime_changed and content_changed)
	create_test_file(&source, "content_change.txt", "MODIFIED")?;

	// Run second backup
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Get second backup stats
	let mut backup_sets: Vec<_> = fs::read_dir(&backup_root)?.filter_map(Result::ok).collect();
	backup_sets.sort_by_key(|entry| entry.path().metadata().unwrap().modified().unwrap());
	let stats = fs::read_to_string(backup_sets[1].path().join("disk-hog-backup-stats.txt"))?;

	// Verify we got one of each type
	println!("Stats:\n{}", stats);
	assert!(
		stats.contains("New:              0"),
		"Should have 0 new files (second backup)"
	);
	assert!(
		stats.contains("Size changed:     1"),
		"Should have 1 size change (size_change.txt)"
	);
	assert!(
		stats.contains("Mtime changed:    2"),
		"Should have 2 mtime changes (mtime_change.txt and content_change.txt)"
	);
	assert!(
		stats.contains("Content changed:  1"),
		"Should have 1 content change (content_change.txt)"
	);
	assert!(
		stats.contains("Hardlinked:       2 files"),
		"Should hardlink 2 files (unchanged.txt, mtime_change.txt)"
	);
	assert!(
		stats.contains("Copied:           2 files"),
		"Should copy 2 files (size_change.txt, content_change.txt)"
	);

	Ok(())
}

#[test]
fn test_disk_space_reporting() -> io::Result<()> {
	// Set up source directory with test files
	let source = create_tmp_folder("disk_space_source")?;
	create_test_file(&source, "file1.txt", "test content 1")?;
	create_test_file(&source, "file2.txt", "test content 2")?;

	// Create backup destination
	let backup_root = create_tmp_folder("disk_space_backups")?;

	// Run backup command and capture output
	let output = disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.output()?;

	// Check that the command succeeded
	assert!(output.status.success(), "Backup command should succeed");

	// Convert stderr to string (all output goes to stderr)
	let stderr = String::from_utf8_lossy(&output.stderr);

	// Verify initial disk space reporting
	assert!(
		stderr.contains("Target disk space before backup:"),
		"Should report disk space before backup"
	);
	assert!(stderr.contains("Total:"), "Should report total disk space");
	assert!(
		stderr.contains("Available:"),
		"Should report available disk space"
	);
	assert!(stderr.contains("Used:"), "Should report used disk space");

	Ok(())
}

#[test]
fn test_auto_delete_flag() -> io::Result<()> {
	// Set up source directory with test files
	let source = create_tmp_folder("auto_delete_source")?;
	create_test_file(&source, "file.txt", "test content")?;

	// Create backup destination
	let backup_root = create_tmp_folder("auto_delete_backups")?;

	// Run backup command WITH --auto-delete flag
	let output = disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.arg("--auto-delete")
		.output()?;

	// Check that the command succeeded
	assert!(
		output.status.success(),
		"Backup with auto-delete should succeed"
	);

	// Verify backup was created
	let sets: Vec<_> = fs::read_dir(&backup_root)?
		.filter_map(Result::ok)
		.filter(|e| e.file_name().to_string_lossy().starts_with("dhb-set-"))
		.collect();
	assert_eq!(sets.len(), 1, "Should have created one backup set");

	// Note: This test doesn't trigger actual disk-full conditions
	// Just-in-time deletion messaging would only appear if disk space was actually low
	// Per ADR-004, we accept the testing gap for E2E disk exhaustion scenarios

	Ok(())
}
