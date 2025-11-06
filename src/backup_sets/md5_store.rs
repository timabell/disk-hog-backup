use md5::Context as Md5Context;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const MD5_FILENAME: &str = "disk-hog-backup-hashes.md5";

pub struct Md5Store {
	hashes: HashMap<PathBuf, [u8; 16]>,
	backup_root: PathBuf,
}

impl Md5Store {
	pub fn new(backup_root: &Path) -> Self {
		Md5Store {
			hashes: HashMap::new(),
			backup_root: backup_root.to_path_buf(),
		}
	}

	pub fn load_from_backup(backup_path: &Path) -> io::Result<Self> {
		let mut store = Self::new(backup_path);
		let md5_file_path = backup_path.join(MD5_FILENAME);

		if md5_file_path.exists() {
			let file = File::open(&md5_file_path)?;
			let reader = BufReader::new(file);

			for line in reader.lines() {
				let line = line?;
				if line.trim().is_empty() || line.starts_with('#') {
					continue;
				}

				if let Some((hash, path)) = Self::parse_md5_line(&line) {
					store.hashes.insert(path, hash);
				}
			}
		}

		Ok(store)
	}

	pub fn save(&self) -> io::Result<()> {
		let md5_file_path = self.backup_root.join(MD5_FILENAME);
		let mut file = File::create(&md5_file_path)?;

		let mut entries: Vec<(&PathBuf, &[u8; 16])> = self.hashes.iter().collect();
		entries.sort_by(|a, b| a.0.cmp(b.0));

		for (path, hash) in entries {
			let path_str = path.to_string_lossy();
			let hash_hex = hash.iter().fold(String::with_capacity(32), |mut acc, &b| {
				write!(acc, "{:02x}", b).unwrap();
				acc
			});

			// Escape special characters in the path using backslash escaping (similar to GNU md5sum)
			// First escape backslashes, then escape newlines and carriage returns
			let escaped_path = path_str
				.replace('\\', "\\\\")
				.replace('\n', "\\n")
				.replace('\r', "\\r");

			// Check if the path contains special characters that need escaping
			let needs_escaping =
				path_str.contains('\\') || path_str.contains('\n') || path_str.contains('\r');

			// Add a leading backslash for filenames with special characters, matching GNU md5sum behavior
			if needs_escaping {
				writeln!(file, "\\{}  {}", hash_hex, escaped_path)?;
			} else {
				writeln!(file, "{}  {}", hash_hex, escaped_path)?;
			}
		}

		eprintln!();
		eprintln!("MD5 hashes saved to: {}", md5_file_path.display());

		Md5Store::create_md5_checksum_of_md5_file(&md5_file_path)?;

		Ok(())
	}

	pub fn add_hash(&mut self, rel_path: &Path, hash: [u8; 16]) {
		self.hashes.insert(rel_path.to_path_buf(), hash);
	}

	pub fn get_hash(&self, rel_path: &Path) -> Option<&[u8; 16]> {
		self.hashes.get(rel_path)
	}
}

impl Md5Store {
	fn parse_md5_line(line: &str) -> Option<([u8; 16], PathBuf)> {
		// Check if the line starts with a backslash (indicating special characters)
		let line = if let Some(stripped) = line.strip_prefix('\\') {
			stripped // Remove the leading backslash
		} else {
			line
		};

		let parts: Vec<&str> = line.splitn(2, "  ").collect();
		if parts.len() != 2 {
			return None;
		}

		let hex_hash = parts[0];
		let escaped_path = parts[1];

		if hex_hash.len() != 32 {
			return None;
		}

		// Parse the hash
		let mut hash = [0u8; 16];
		for i in 0..16 {
			let byte_str = &hex_hash[i * 2..i * 2 + 2];
			match u8::from_str_radix(byte_str, 16) {
				Ok(byte) => hash[i] = byte,
				Err(_) => return None,
			}
		}

		// Unescape the path
		let unescaped_path = Self::unescape_path(escaped_path);

		Some((hash, PathBuf::from(unescaped_path)))
	}

	fn unescape_path(escaped_path: &str) -> String {
		let mut unescaped_path = String::new();
		let mut i = 0;
		let chars: Vec<char> = escaped_path.chars().collect();

		while i < chars.len() {
			let c = chars[i];
			if c == '\\' && i + 1 < chars.len() {
				// Check for escaped characters
				let next_char = chars[i + 1];
				match next_char {
					'n' => {
						unescaped_path.push('\n');
						i += 2;
					}
					'r' => {
						unescaped_path.push('\r');
						i += 2;
					}
					'\\' => {
						unescaped_path.push('\\');
						i += 2;
					}
					_ => {
						// Not a special escape sequence, just a backslash
						unescaped_path.push('\\');
						i += 1;
					}
				}
			} else {
				unescaped_path.push(c);
				i += 1;
			}
		}

		unescaped_path
	}

	fn create_md5_checksum_of_md5_file(md5_file_path: &PathBuf) -> io::Result<()> {
		let md5_checksum_path = md5_file_path.with_file_name(format!("{}.md5", MD5_FILENAME));

		let md5_content = std::fs::read(md5_file_path)?;

		let mut hasher = Md5Context::new();
		hasher.consume(&md5_content);
		let result = hasher.finalize();

		let hash_hex = result
			.iter()
			.fold(String::with_capacity(32), |mut acc, &b| {
				write!(acc, "{:02x}", b).unwrap();
				acc
			});

		let mut file = File::create(&md5_checksum_path)?;
		writeln!(file, "{}  {}", hash_hex, MD5_FILENAME)?;

		eprintln!("MD5 checksum saved to: {}", md5_checksum_path.display());

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;
	use tempfile::tempdir;

	#[test]
	fn test_md5_store_save_and_load() {
		let temp_dir = tempdir().unwrap();
		let backup_path = temp_dir.path();

		let mut store = Md5Store::new(backup_path);

		let hash1 = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
		let hash2 = [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];

		store.add_hash(Path::new("file1.txt"), hash1);
		store.add_hash(Path::new("dir/file2.txt"), hash2);

		store.save().unwrap();

		let md5_checksum_path = backup_path.join(format!("{}.md5", MD5_FILENAME));
		assert!(md5_checksum_path.exists(), "MD5 checksum file should exist");

		let content = std::fs::read_to_string(&md5_checksum_path).unwrap();
		assert!(
			content.ends_with(&format!("  {}\n", MD5_FILENAME)),
			"MD5 checksum file should be in md5sum compatible format"
		);
		assert_eq!(
			content.len(),
			32 + 2 + MD5_FILENAME.len() + 1,
			"MD5 checksum file should contain a 32-character hash, two spaces, the filename, and a newline"
		);

		let loaded_store = Md5Store::load_from_backup(backup_path).unwrap();

		assert_eq!(loaded_store.get_hash(Path::new("file1.txt")), Some(&hash1));
		assert_eq!(
			loaded_store.get_hash(Path::new("dir/file2.txt")),
			Some(&hash2)
		);
		assert_eq!(loaded_store.get_hash(Path::new("nonexistent.txt")), None);

		assert_eq!(loaded_store.get_hash(Path::new("file1.txt")), Some(&hash1));
		assert_ne!(loaded_store.get_hash(Path::new("file1.txt")), Some(&hash2));
	}

	#[test]
	fn test_md5_store_with_invalid_data() {
		let temp_dir = tempdir().unwrap();
		let backup_path = temp_dir.path();
		let md5_file_path = backup_path.join(MD5_FILENAME);

		{
			let mut file = File::create(&md5_file_path).unwrap();
			writeln!(file, "# Comment line").unwrap();
			writeln!(file).unwrap();
			writeln!(file, "0102030405060708090a0b0c0d0e0f10  valid_file.txt").unwrap();
			writeln!(file, "invalid_hash  file_with_invalid_hash.txt").unwrap();
			writeln!(file, "0102030405060708090a0b0c0d0e0f10").unwrap();
		}

		let store = Md5Store::load_from_backup(backup_path).unwrap();

		assert!(store.get_hash(Path::new("valid_file.txt")).is_some());
		assert!(
			store
				.get_hash(Path::new("file_with_invalid_hash.txt"))
				.is_none()
		);
	}

	#[test]
	fn test_md5_checksum_of_md5_file() {
		let temp_dir = tempdir().unwrap();
		let backup_path = temp_dir.path();

		let mut store = Md5Store::new(backup_path);

		let hash = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
		store.add_hash(Path::new("file.txt"), hash);

		store.save().unwrap();

		let md5_file_path = backup_path.join(MD5_FILENAME);
		let md5_checksum_path = backup_path.join(format!("{}.md5", MD5_FILENAME));

		assert!(md5_file_path.exists(), "MD5 file should exist");
		assert!(md5_checksum_path.exists(), "MD5 checksum file should exist");

		let md5_content = std::fs::read(&md5_file_path).unwrap();
		let mut hasher = Md5Context::new();
		hasher.consume(&md5_content);
		let result = hasher.finalize();

		let expected_hash = result
			.iter()
			.fold(String::with_capacity(32), |mut acc, &b| {
				write!(acc, "{:02x}", b).unwrap();
				acc
			});

		let checksum_content = std::fs::read_to_string(&md5_checksum_path).unwrap();
		let actual_hash = checksum_content.split("  ").next().unwrap();

		assert_eq!(
			expected_hash, actual_hash,
			"MD5 checksum should match the actual hash of the MD5 file"
		);
	}

	#[test]
	fn test_special_characters_in_filenames() {
		let temp_dir = tempdir().unwrap();
		let backup_path = temp_dir.path();

		let mut store = Md5Store::new(backup_path);

		let hash1 = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
		let path1 = PathBuf::from("file\nwith\nnewlines.txt");

		let hash2 = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17];
		let path2 = PathBuf::from("file\\with\\backslashes.txt");

		let hash3 = [3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18];
		let path3 = PathBuf::from("file\rwith\rcarriage\rreturns.txt");

		// Add hashes to the store
		store.add_hash(&path1, hash1);
		store.add_hash(&path2, hash2);
		store.add_hash(&path3, hash3);

		// Save the store
		store.save().unwrap();

		// Define expected content using a raw string
		let expected_content = r#"\0102030405060708090a0b0c0d0e0f10  file\nwith\nnewlines.txt
\030405060708090a0b0c0d0e0f101112  file\rwith\rcarriage\rreturns.txt
\02030405060708090a0b0c0d0e0f1011  file\\with\\backslashes.txt
"#;

		// Read the actual content
		let md5_file_path = backup_path.join(MD5_FILENAME);
		let actual_content = std::fs::read_to_string(&md5_file_path).unwrap();

		// Assert that the content matches
		assert_eq!(expected_content, actual_content);

		// Load the store from backup
		let loaded_store = Md5Store::load_from_backup(backup_path).unwrap();

		// Verify that the hashes are correctly loaded
		assert_eq!(
			loaded_store.get_hash(&path1),
			Some(&hash1),
			"Failed to find hash for path with newlines"
		);
		assert_eq!(
			loaded_store.get_hash(&path2),
			Some(&hash2),
			"Failed to find hash for path with backslashes"
		);
		assert_eq!(
			loaded_store.get_hash(&path3),
			Some(&hash3),
			"Failed to find hash for path with carriage returns"
		);
	}
}
