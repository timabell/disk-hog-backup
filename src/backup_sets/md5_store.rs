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

				let parts: Vec<&str> = line.splitn(2, "  ").collect();
				if parts.len() == 2 {
					let hex_hash = parts[0];
					let rel_path = parts[1];

					if hex_hash.len() == 32 {
						let mut hash = [0u8; 16];
						for i in 0..16 {
							let byte_str = &hex_hash[i * 2..i * 2 + 2];
							if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
								hash[i] = byte;
							} else {
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

			writeln!(file, "{}  {}", hash_hex, path_str)?;
		}

		self.create_md5_checksum_of_md5_file()?;

		Ok(())
	}

	fn create_md5_checksum_of_md5_file(&self) -> io::Result<()> {
		let md5_file_path = self.backup_root.join(MD5_FILENAME);
		let md5_checksum_path = self.backup_root.join(format!("{}.md5", MD5_FILENAME));

		let md5_content = std::fs::read(&md5_file_path)?;

		let mut hasher = Md5Context::new();
		hasher.consume(&md5_content);
		let result = hasher.compute();

		let hash_hex = result
			.iter()
			.fold(String::with_capacity(32), |mut acc, &b| {
				write!(acc, "{:02x}", b).unwrap();
				acc
			});

		let mut file = File::create(md5_checksum_path)?;
		writeln!(file, "{}  {}", hash_hex, MD5_FILENAME)?;

		Ok(())
	}

	pub fn add_hash(&mut self, rel_path: &Path, hash: [u8; 16]) {
		self.hashes.insert(rel_path.to_path_buf(), hash);
	}

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
			writeln!(file, "").unwrap();
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
		let result = hasher.compute();

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
}
