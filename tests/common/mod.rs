use rand::Rng;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

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
	std::io::Write::write_all(&mut file, contents.as_bytes())?;
	Ok(())
}
