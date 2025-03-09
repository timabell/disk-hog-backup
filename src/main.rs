mod backup;
mod backup_sets;
mod dhcopy;

use crate::backup::backup;
use crate::backup_sets::md5_store::Md5Store;
use clap::Parser;
use std::fs;
use std::path::Path;
use std::process;

#[derive(Parser)]
#[command(about = "A tool for backing up directories", long_about = None)]
#[clap(author, version)]
struct Args {
	/// Source folder to back up
	#[arg(short, long, required_unless_present = "verify")]
	source: Option<String>,

	/// Destination folder for backups
	#[arg(short, long, required_unless_present = "verify")]
	destination: Option<String>,

	/// Verify a backup set against its stored hashes
	#[arg(long)]
	verify: Option<String>,
}

fn main() {
	println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
	println!("License: {}", env!("CARGO_PKG_LICENSE"));
	println!("{}", env!("CARGO_PKG_REPOSITORY"));
	println!();

	let args = Args::parse();

	if let Some(backup_set_path) = args.verify {
		// Handle verify command
		match verify_backup_set(&backup_set_path) {
			Ok(true) => {
				println!("✓ Verification successful: All files match their stored hashes");
			}
			Ok(false) => {
				eprintln!("✗ Verification failed: Some files do not match their stored hashes");
				process::exit(1);
			}
			Err(e) => {
				eprintln!("Verification failed: {}", e);
				process::exit(1);
			}
		}
	} else {
		// Handle backup command
		let source = args.source.unwrap();
		let destination = args.destination.unwrap();
		
		match backup(&source, &destination) {
			Ok(_) => (),
			Err(e) => {
				eprintln!("Backup failed: {}", e);
				process::exit(1);
			}
		}
	}
}

/// Verify a backup set against its stored hashes
fn verify_backup_set(backup_set_path: &str) -> std::io::Result<bool> {
	println!("Verifying backup set: {}", backup_set_path);
	
	// Load the MD5 store from the backup set
	let md5_store = Md5Store::load_from_backup(Path::new(backup_set_path))?;
	
	let mut all_files_match = true;
	let mut verified_count = 0;
	let mut failed_count = 0;
	
	// Walk through all files in the backup set
	let backup_set_path = Path::new(backup_set_path);
	visit_dirs(backup_set_path, backup_set_path, &md5_store, &mut verified_count, &mut failed_count, &mut all_files_match)?;
	
	println!("Verification complete: {} files verified, {} files failed", verified_count, failed_count);
	
	Ok(all_files_match)
}

/// Recursively visit directories and verify files against their stored hashes
fn visit_dirs(
	base_path: &Path,
	dir_path: &Path,
	md5_store: &Md5Store,
	verified_count: &mut usize,
	failed_count: &mut usize,
	all_files_match: &mut bool,
) -> std::io::Result<()> {
	if dir_path.is_dir() {
		for entry in fs::read_dir(dir_path)? {
			let entry = entry?;
			let path = entry.path();
			
			if path.is_dir() {
				visit_dirs(base_path, &path, md5_store, verified_count, failed_count, all_files_match)?;
			} else if path.is_file() && !path.file_name().unwrap_or_default().to_string_lossy().starts_with("backup_md5_hashes") {
				// Skip the MD5 hash file itself
				let rel_path = path.strip_prefix(base_path).unwrap_or(&path);
				
				// Verify the file against its stored hash
				match verify_file(&path, rel_path, md5_store) {
					Ok(true) => {
						*verified_count += 1;
					}
					Ok(false) => {
						eprintln!("✗ File hash mismatch: {}", rel_path.display());
						*failed_count += 1;
						*all_files_match = false;
					}
					Err(e) => {
						eprintln!("Error verifying file {}: {}", rel_path.display(), e);
						*failed_count += 1;
						*all_files_match = false;
					}
				}
			}
		}
	}
	
	Ok(())
}

/// Verify a single file against its stored hash
fn verify_file(file_path: &Path, rel_path: &Path, md5_store: &Md5Store) -> std::io::Result<bool> {
	// Get the stored hash for this file
	if let Some(stored_hash) = md5_store.get_hash(rel_path) {
		// Calculate the current hash of the file
		let current_hash = dhcopy::streaming_copy::calculate_md5(file_path)?;
		
		// Compare the hashes
		Ok(current_hash == *stored_hash)
	} else {
		// No stored hash found for this file
		eprintln!("No stored hash found for file: {}", rel_path.display());
		Ok(false)
	}
}
