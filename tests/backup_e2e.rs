// This is an integration test file that lives in the tests/ directory
// Integration tests verify that multiple components work together correctly
mod common;  // Helper module for shared test utilities

use common::{create_test_folder};  // Import test utilities
use std::process::Command;  // For executing CLI commands

#[test]
fn test_backup_via_cli() -> Result<(), Box<dyn std::error::Error>> {
    // Create temporary test directories using helper from common/mod.rs
    let source_dir = create_test_folder();
    let dest_dir = create_test_folder();
    
    // Test the CLI by actually running the binary with cargo run
    // This tests the full application stack from CLI to core logic
    let status = Command::new("cargo")
        .args([
            "run",
            "--",  // Arguments after -- are passed to our program
            "--source", source_dir.path().to_str().unwrap(),
            "--destination", dest_dir.path().to_str().unwrap()
        ])
        .status()?;

    assert!(status.success());
    Ok(())
}