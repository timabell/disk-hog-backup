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

/// Parse tab-separated stats file into a HashMap
fn parse_stats_file(content: &str) -> std::collections::HashMap<String, String> {
	let mut stats = std::collections::HashMap::new();
	for line in content.lines().skip(1) {
		// Skip header
		if let Some((key, rest)) = line.split_once('\t')
			&& let Some((bytes, _human)) = rest.split_once('\t')
		{
			stats.insert(key.to_string(), bytes.to_string());
		}
	}
	stats
}

#[test]
fn test_backup_stats_functionality() -> io::Result<()> {
	// Set up source directory with multiple test files of different sizes
	let source = create_tmp_folder("stats_source")?;

	// Create different types of files to test stats
	create_test_file(&source, "small.txt", "Small file content")?; // ~18 bytes
	create_test_file(&source, "medium.txt", &"x".repeat(1024))?; // 1KB
	create_test_file(&source, "large.txt", &"y".repeat(10 * 1024))?; // 10KB
	create_test_file(&source, "nested/deep.txt", "Nested file")?; // ~11 bytes

	// Create backup destination
	let backup_root = create_tmp_folder("stats_backups")?;

	// Run first backup
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Get the backup set directory (should be only one)
	let backup_sets: Vec<_> = fs::read_dir(&backup_root)?.filter_map(Result::ok).collect();
	assert_eq!(backup_sets.len(), 1, "Should have exactly one backup set");

	let backup_set_path = backup_sets[0].path();

	// Verify that the stats file was created
	let stats_file = backup_set_path.join("disk-hog-backup-stats.txt");
	assert!(
		stats_file.exists(),
		"Stats file should exist: {}",
		stats_file.display()
	);

	// Read and parse the stats file
	let stats_content = fs::read_to_string(&stats_file)?;
	let stats = parse_stats_file(&stats_content);

	// Verify basic stats structure and values
	let processing_time: f64 = stats
		.get("processing_time_seconds")
		.expect("Should have processing time")
		.parse()
		.expect("Processing time should be a number");
	assert!(
		processing_time >= 0.0,
		"Processing time should be non-negative"
	);

	let files_processed: u64 = stats
		.get("files_processed")
		.expect("Should have files processed")
		.parse()
		.expect("Files processed should be a number");
	assert_eq!(files_processed, 4, "Should have processed 4 files");

	let files_hardlinked: u64 = stats
		.get("files_hardlinked")
		.expect("Should have files hardlinked")
		.parse()
		.expect("Files hardlinked should be a number");
	assert_eq!(files_hardlinked, 0, "First backup should have no hardlinks");

	let files_copied: u64 = stats
		.get("files_copied")
		.expect("Should have files copied")
		.parse()
		.expect("Files copied should be a number");
	assert_eq!(
		files_copied, 4,
		"All 4 files should be copied in first backup"
	);

	// Verify byte counts
	let expected_total_bytes = 18 + 1024 + (10 * 1024) + 11; // Sum of file sizes

	let bytes_processed: u64 = stats
		.get("bytes_processed")
		.expect("Should have bytes processed")
		.parse()
		.expect("Bytes processed should be a number");
	assert_eq!(
		bytes_processed, expected_total_bytes as u64,
		"Should process expected total bytes"
	);

	let bytes_written: u64 = stats
		.get("bytes_written_new_data")
		.expect("Should have bytes written")
		.parse()
		.expect("Bytes written should be a number");
	assert_eq!(
		bytes_written, expected_total_bytes as u64,
		"Should write all bytes in first backup"
	);

	let bytes_saved: u64 = stats
		.get("bytes_saved_via_hardlink")
		.expect("Should have bytes saved")
		.parse()
		.expect("Bytes saved should be a number");
	assert_eq!(bytes_saved, 0, "Should save no bytes in first backup");

	// Verify speed calculation
	let speed: f64 = stats
		.get("processing_speed_bytes_per_sec")
		.expect("Should have processing speed")
		.parse()
		.expect("Processing speed should be a number");
	assert!(speed > 0.0, "Processing speed should be positive");

	// Sleep to ensure different backup set names (they're based on timestamp)
	thread::sleep(time::Duration::from_secs(2));

	// Now run a second backup to test hardlinking stats
	disk_hog_backup_cmd()
		.arg("--source")
		.arg(&source)
		.arg("--destination")
		.arg(&backup_root)
		.assert()
		.success();

	// Get the backup sets again
	let backup_sets: Vec<_> = fs::read_dir(&backup_root)?.filter_map(Result::ok).collect();
	assert_eq!(
		backup_sets.len(),
		2,
		"Should have exactly two backup sets after second backup"
	);

	// Find the newer backup set (by modification time)
	let mut backup_set_paths: Vec<_> = backup_sets
		.iter()
		.map(|entry| {
			let path = entry.path();
			let metadata = path.metadata().unwrap();
			(path, metadata.modified().unwrap())
		})
		.collect();
	backup_set_paths.sort_by_key(|(_, modified)| *modified);
	let second_backup_path = &backup_set_paths[1].0;

	// Read stats from second backup
	let second_stats_file = second_backup_path.join("disk-hog-backup-stats.txt");
	assert!(
		second_stats_file.exists(),
		"Second backup stats file should exist"
	);

	let second_stats_content = fs::read_to_string(&second_stats_file)?;
	let second_stats = parse_stats_file(&second_stats_content);

	// Verify hardlinking worked in second backup
	let second_files_processed: u64 = second_stats
		.get("files_processed")
		.expect("Should have files processed")
		.parse()
		.expect("Files processed should be a number");
	assert_eq!(
		second_files_processed, 4,
		"Second backup should process 4 files"
	);

	let second_files_hardlinked: u64 = second_stats
		.get("files_hardlinked")
		.expect("Should have files hardlinked")
		.parse()
		.expect("Files hardlinked should be a number");
	assert_eq!(
		second_files_hardlinked, 4,
		"All files should be hardlinked in second backup"
	);

	let second_files_copied: u64 = second_stats
		.get("files_copied")
		.expect("Should have files copied")
		.parse()
		.expect("Files copied should be a number");
	assert_eq!(
		second_files_copied, 0,
		"No files should be copied in second backup"
	);

	// Verify space savings
	let second_bytes_processed: u64 = second_stats
		.get("bytes_processed")
		.expect("Should have bytes processed")
		.parse()
		.expect("Bytes processed should be a number");
	assert_eq!(
		second_bytes_processed, expected_total_bytes as u64,
		"Should process same total bytes"
	);

	let second_bytes_saved: u64 = second_stats
		.get("bytes_saved_via_hardlink")
		.expect("Should have bytes saved")
		.parse()
		.expect("Bytes saved should be a number");
	assert_eq!(
		second_bytes_saved, expected_total_bytes as u64,
		"Should save all bytes via hardlinking"
	);

	let second_bytes_written: u64 = second_stats
		.get("bytes_written_new_data")
		.expect("Should have bytes written")
		.parse()
		.expect("Bytes written should be a number");
	assert_eq!(
		second_bytes_written, 0,
		"Should write no new bytes in second backup"
	);

	println!("✓ Stats functionality test passed!");
	println!(
		"  First backup: {} files copied, {} bytes written",
		files_copied, bytes_written
	);
	println!(
		"  Second backup: {} files hardlinked, {} bytes saved",
		second_files_hardlinked, second_bytes_saved
	);

	Ok(())
}
