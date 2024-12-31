use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use chrono::{DateTime, Utc};

pub fn create_tmp_folder(prefix: &str) -> io::Result<String> {
    let dir = std::env::temp_dir().join(format!("dhb-{}-", prefix));
    fs::create_dir_all(&dir)?;
    Ok(dir.to_string_lossy().into_owned())
}

pub fn file_contents_matches(file1_path: &str, file2_path: &str) -> io::Result<bool> {
    let file1_contents = read_contents(file1_path)?;
    let file2_contents = read_contents(file2_path)?;
    Ok(file1_contents == file2_contents)
}

fn read_contents<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

// Returns a function that always returns the same time
pub fn time_fixer() -> impl Fn() -> DateTime<Utc> {
    let fixed_time = Utc::now();
    move || fixed_time
}
