use md5::Context;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub fn calculate_md5(path: &Path) -> io::Result<[u8; 16]> {
	let mut file = File::open(path)?;
	let mut context = Context::new();
	let mut buffer = [0; 8192]; // 8KB buffer for reading

	loop {
		let bytes_read = file.read(&mut buffer)?;
		if bytes_read == 0 {
			break;
		}
		context.consume(&buffer[..bytes_read]);
	}

	let digest = context.compute();
	Ok(digest.0)
}

pub fn files_match(path1: &Path, path2: &Path) -> io::Result<bool> {
	// First check if file sizes match
	let metadata1 = path1.metadata()?;
	let metadata2 = path2.metadata()?;

	if metadata1.len() != metadata2.len() {
		return Ok(false);
	}

	// Then compare MD5 hashes
	let hash1 = calculate_md5(path1)?;
	let hash2 = calculate_md5(path2)?;

	Ok(hash1 == hash2)
}
