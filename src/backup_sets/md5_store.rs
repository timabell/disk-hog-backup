use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const MD5_FILENAME: &str = "backup_md5_hashes.txt";

pub struct Md5Store {
	hashes: HashMap<PathBuf, [u8; 16]>,
	backup_root: PathBuf,
}

impl Md5Store {
	/// Create a new MD5 store for a backup set
	pub fn new(backup_root: &Path) -> Self {
		Md5Store {
			hashes: HashMap::new(),
			backup_root: backup_root.to_path_buf(),
		}
	}

	/// Load MD5 hashes from a previous backup set
	pub fn load_from_backup(backup_path: &Path) -> io::Result<Self> {
		let mut store = Self::new(backup_path);
		let md5_file_path = backup_path.join(MD5_FILENAME);

		if md5_file_path.exists() {
			let file = File::open(&md5_file_path)?;
			let reader = BufReader::new(file);

			for line in reader.lines() {
				let line = line?;
				if line.trim().is_empty() || line.starts_with('#') {
					continue; // Skip empty lines and comments
				}

				// Format: <hex_md5_hash> <relative_path>
				let parts: Vec<&str> = line.splitn(2, ' ').collect();
				if parts.len() == 2 {
					let hex_hash = parts[0];
					let rel_path = parts[1];

					if hex_hash.len() == 32 {
						// Convert hex string to [u8; 16]
						let mut hash = [0u8; 16];
						for i in 0..16 {
							let byte_str = &hex_hash[i * 2..i * 2 + 2];
							if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
								hash[i] = byte;
							} else {
								// Invalid hex string, skip this entry
								continue;
							}
						}

						store.hashes.insert(PathBuf::from(rel_path), hash);
					}
				}
			}
		}

		Ok(store)
	}

	/// Save MD5 hashes to the backup set
	pub fn save(&self) -> io::Result<()> {
		let md5_file_path = self.backup_root.join(MD5_FILENAME);
		let mut file = File::create(&md5_file_path)?;

		// Write header
		writeln!(file, "# Backup MD5 hashes - DO NOT EDIT")?;
		writeln!(file, "# Format: <md5_hash_hex> <relative_path>")?;
		writeln!(file)?;

		// Write entries
		for (path, hash) in &self.hashes {
			let path_str = path.to_string_lossy();
			let hash_hex = hash.iter().fold(String::with_capacity(32), |mut acc, &b| {
				write!(acc, "{:02x}", b).unwrap();
				acc
			});

			writeln!(file, "{} {}", hash_hex, path_str)?;
		}

		Ok(())
	}

	/// Add or update a hash for a file
	pub fn add_hash(&mut self, rel_path: &Path, hash: [u8; 16]) {
		self.hashes.insert(rel_path.to_path_buf(), hash);
	}

	/// Get the hash for a file if it exists
	pub fn get_hash(&self, rel_path: &Path) -> Option<&[u8; 16]> {
		self.hashes.get(rel_path)
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

		// Create a new store
		let mut store = Md5Store::new(backup_path);

		// Add some hashes
		let hash1 = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
		let hash2 = [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];

		store.add_hash(Path::new("file1.txt"), hash1);
		store.add_hash(Path::new("dir/file2.txt"), hash2);

		// Save the store
		store.save().unwrap();

		// Load the store
		let loaded_store = Md5Store::load_from_backup(backup_path).unwrap();

		// Check that the hashes match
		assert_eq!(loaded_store.get_hash(Path::new("file1.txt")), Some(&hash1));
		assert_eq!(
			loaded_store.get_hash(Path::new("dir/file2.txt")),
			Some(&hash2)
		);
		assert_eq!(loaded_store.get_hash(Path::new("nonexistent.txt")), None);

		// Check hash matching manually
		assert_eq!(loaded_store.get_hash(Path::new("file1.txt")), Some(&hash1));
		assert_ne!(loaded_store.get_hash(Path::new("file1.txt")), Some(&hash2));
	}

	#[test]
	fn test_md5_store_with_invalid_data() {
		let temp_dir = tempdir().unwrap();
		let backup_path = temp_dir.path();
		let md5_file_path = backup_path.join(MD5_FILENAME);

		// Create an MD5 file with some invalid entries
		{
			let mut file = File::create(&md5_file_path).unwrap();
			writeln!(file, "# Comment line").unwrap();
			writeln!(file, "").unwrap();
			writeln!(file, "0102030405060708090a0b0c0d0e0f10 valid_file.txt").unwrap();
			writeln!(file, "invalid_hash file_with_invalid_hash.txt").unwrap();
			writeln!(file, "0102030405060708090a0b0c0d0e0f10").unwrap(); // Missing path
		}

		// Load the store
		let store = Md5Store::load_from_backup(backup_path).unwrap();

		// Check that only the valid entry was loaded
		assert!(store.get_hash(Path::new("valid_file.txt")).is_some());
		assert!(
			store
				.get_hash(Path::new("file_with_invalid_hash.txt"))
				.is_none()
		);
	}
}
