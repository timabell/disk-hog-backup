// Key imports
use std::path::Path;         // For path manipulation
use std::fs;                 // File system operations
use tempfile::TempDir;       // For temporary test directories

// Creates temporary test directory that auto-cleans up
pub fn create_test_folder() -> TempDir {
    TempDir::new().unwrap()  // Creates and returns temp directory
}

// Compares contents of two files
pub fn compare_files(file1: &Path, file2: &Path) -> bool {
    // Read both files to strings
    let content1 = fs::read_to_string(file1).unwrap();
    let content2 = fs::read_to_string(file2).unwrap();
    // Compare contents
    content1 == content2
}