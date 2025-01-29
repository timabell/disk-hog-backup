mod common;

use common::{create_test_folder, compare_files};
use std::process::Command;
use std::path::Path;

#[test]
fn test_backup_via_cli() -> Result<(), Box<dyn std::error::Error>> {
    let source_dir = create_test_folder();
    let dest_dir = create_test_folder();
    
    // Run backup command
    let status = Command::new("cargo")
        .args([
            "run",
            "--",
            "--source", source_dir.path().to_str().unwrap(),
            "--destination", dest_dir.path().to_str().unwrap()
        ])
        .status()?;

    assert!(status.success());
    Ok(())
}