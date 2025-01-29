use std::path::Path;
use std::fs;
use tempfile::TempDir;

pub fn create_test_folder() -> TempDir {
    TempDir::new().unwrap()
}

pub fn compare_files(file1: &Path, file2: &Path) -> bool {
    let content1 = fs::read_to_string(file1).unwrap();
    let content2 = fs::read_to_string(file2).unwrap();
    content1 == content2
}