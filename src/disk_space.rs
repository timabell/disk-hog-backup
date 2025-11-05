use std::io;
use std::path::Path;
use sysinfo::Disks;

/// Information about disk space
#[derive(Debug, Clone, Copy)]
pub struct DiskSpace {
	/// Total space in bytes
	pub total: u64,
	/// Available space in bytes
	pub available: u64,
	/// Used space in bytes (total - available)
	pub used: u64,
}

impl DiskSpace {
	/// Create a new DiskSpace instance
	pub fn new(total: u64, available: u64) -> Self {
		DiskSpace {
			total,
			available,
			used: total.saturating_sub(available),
		}
	}

	/// Get the difference in used space compared to another DiskSpace
	pub fn used_difference(&self, other: &DiskSpace) -> i64 {
		self.used as i64 - other.used as i64
	}
}

/// Get disk space information for a given path
///
/// This function uses the sysinfo crate to query disk space in a cross-platform way.
/// It finds the disk that contains the given path by matching mount points, using the
/// longest matching prefix to handle nested mounts correctly.
///
/// # Arguments
/// * `path` - The path to query disk space for (can be a file or directory)
///
/// # Returns
/// * `Ok(DiskSpace)` - Disk space information for the filesystem containing the path
/// * `Err(io::Error)` - If the path doesn't exist or no matching disk is found
///
/// # Examples
/// ```no_run
/// use std::path::Path;
/// use disk_hog_backup::disk_space::get_disk_space;
///
/// let space = get_disk_space(Path::new("/home/user/backups"))?;
/// println!("Available: {} bytes", space.available);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn get_disk_space(path: &Path) -> io::Result<DiskSpace> {
	// Get all mounted disks
	let disks = Disks::new_with_refreshed_list();

	// Canonicalize the path to resolve symlinks and make it absolute
	// If the path doesn't exist, try parent directories until we find one that does
	let canonical_path = match path.canonicalize() {
		Ok(p) => p,
		Err(_) => {
			// Path doesn't exist yet, try to find an existing parent
			let mut parent = path;
			loop {
				if let Some(p) = parent.parent() {
					if let Ok(canonical) = p.canonicalize() {
						break canonical;
					}
					parent = p;
				} else {
					return Err(io::Error::new(
						io::ErrorKind::NotFound,
						format!("Cannot find existing parent for path: {:?}", path),
					));
				}
			}
		}
	};

	// Find the disk with the longest matching mount point
	// This handles nested mounts correctly (e.g., /, /home, /home/user/external)
	let disk = disks
		.iter()
		.filter(|disk| canonical_path.starts_with(disk.mount_point()))
		.max_by_key(|disk| {
			// Use the length of the mount point to find the longest match
			disk.mount_point().as_os_str().len()
		})
		.ok_or_else(|| {
			io::Error::new(
				io::ErrorKind::NotFound,
				format!("No disk found for path: {:?}", path),
			)
		})?;

	Ok(DiskSpace::new(disk.total_space(), disk.available_space()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::env;

	#[test]
	fn test_get_disk_space_current_dir() -> io::Result<()> {
		let current_dir = env::current_dir()?;
		let space = get_disk_space(&current_dir)?;

		// Basic sanity checks
		assert!(space.total > 0, "Total space should be greater than 0");
		assert!(
			space.available <= space.total,
			"Available space should not exceed total space"
		);
		assert!(
			space.used <= space.total,
			"Used space should not exceed total space"
		);
		assert_eq!(
			space.used,
			space.total - space.available,
			"Used space should equal total minus available"
		);

		Ok(())
	}

	#[test]
	fn test_get_disk_space_nonexistent_path() {
		// For a path that doesn't exist, it should still find disk space for the parent
		// (or fail gracefully if no parent exists)
		let result = get_disk_space(Path::new("/nonexistent/path/that/should/not/exist"));

		// On most systems this will succeed by finding the root filesystem
		// On some systems it might fail - either way is acceptable
		match result {
			Ok(space) => {
				assert!(space.total > 0, "Should have valid disk space");
			}
			Err(e) => {
				assert_eq!(e.kind(), io::ErrorKind::NotFound);
			}
		}
	}

	#[test]
	fn test_disk_space_difference() {
		let before = DiskSpace::new(1000, 600); // 400 used
		let after = DiskSpace::new(1000, 500); // 500 used

		assert_eq!(after.used_difference(&before), 100);
		assert_eq!(before.used_difference(&after), -100);
	}

	#[test]
	fn test_disk_space_new() {
		let space = DiskSpace::new(1000, 600);
		assert_eq!(space.total, 1000);
		assert_eq!(space.available, 600);
		assert_eq!(space.used, 400);
	}

	#[test]
	fn test_disk_space_saturating_sub() {
		// Test edge case where available > total (shouldn't happen but let's be safe)
		let space = DiskSpace::new(100, 200);
		assert_eq!(space.used, 0); // Should saturate to 0, not overflow
	}
}
