use std::io;
use std::path::Path;
use sysinfo::{Disk, Disks};

/// Trait for checking disk space - allows testing with mocked space values
pub trait SpaceChecker {
	fn get_available_space(&self, path: &Path) -> io::Result<u64>;
}

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

/// Find the mount point with the longest match for the given path.
///
/// When multiple mount points could match a path (e.g., both / and /mnt/backup match
/// /mnt/backup/files), we must select the longest one to get the correct disk.
/// This ensures we check space on the actual target disk, not a parent mount.
///
/// # Arguments
///
/// * `mount_points` - Iterator of mount point paths to search
/// * `path` - The canonical path to match against mount points
///
/// # Returns
///
/// Returns the longest matching mount point, or None if no mount point matches.
fn find_longest_matching_mount<'a, I>(mount_points: I, path: &Path) -> Option<&'a Path>
where
	I: Iterator<Item = &'a Path>,
{
	mount_points
		.filter(|mount| path.starts_with(mount))
		.max_by_key(|mount| mount.as_os_str().len())
}

/// Find the disk with the longest matching mount point for the given path.
fn find_disk_for_path<'a>(disks: impl Iterator<Item = &'a Disk>, path: &Path) -> Option<&'a Disk> {
	let disks: Vec<_> = disks.collect();
	let mount_points: Vec<_> = disks.iter().map(|d| d.mount_point()).collect();

	find_longest_matching_mount(mount_points.iter().copied(), path)?;

	disks
		.into_iter()
		.filter(|disk| path.starts_with(disk.mount_point()))
		.max_by_key(|disk| disk.mount_point().as_os_str().len())
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
	let canonical_path = path.canonicalize()?;

	let disk = find_disk_for_path(disks.iter(), &canonical_path).ok_or_else(|| {
		io::Error::new(
			io::ErrorKind::NotFound,
			format!("No disk found for path: {:?}", path),
		)
	})?;

	Ok(DiskSpace::new(disk.total_space(), disk.available_space()))
}

/// Production implementation using sysinfo crate
pub struct RealSpaceChecker;

impl SpaceChecker for RealSpaceChecker {
	fn get_available_space(&self, path: &Path) -> io::Result<u64> {
		get_disk_space(path).map(|ds| ds.available)
	}
}

/// Test implementation returning controlled values
#[cfg(test)]
pub struct MockSpaceChecker {
	pub available: u64,
}

#[cfg(test)]
impl SpaceChecker for MockSpaceChecker {
	fn get_available_space(&self, _path: &Path) -> io::Result<u64> {
		Ok(self.available)
	}
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
	fn test_find_longest_matching_mount() {
		// Simulate typical backup scenario: root disk and external USB drive
		let mounts = [Path::new("/"), Path::new("/media/backup-drive")];

		// Backup to external drive should match the external mount
		let result = find_longest_matching_mount(
			mounts.iter().copied(),
			Path::new("/media/backup-drive/backups/2024-01-15"),
		);
		assert_eq!(result, Some(Path::new("/media/backup-drive")));

		// Backup to home directory should match root
		let result =
			find_longest_matching_mount(mounts.iter().copied(), Path::new("/home/user/backups"));
		assert_eq!(result, Some(Path::new("/")));

		// Multiple nested external mounts
		let nested_mounts = [
			Path::new("/"),
			Path::new("/mnt"),
			Path::new("/mnt/external-hdd"),
		];
		let result = find_longest_matching_mount(
			nested_mounts.iter().copied(),
			Path::new("/mnt/external-hdd/backups/photos"),
		);
		assert_eq!(result, Some(Path::new("/mnt/external-hdd")));

		// Empty mounts
		let empty: Vec<&Path> = vec![];
		let result = find_longest_matching_mount(empty.iter().copied(), Path::new("/home/backups"));
		assert_eq!(result, None);
	}

	#[test]
	fn test_disk_space_saturating_sub() {
		// Test edge case where available > total (shouldn't happen but let's be safe)
		let space = DiskSpace::new(100, 200);
		assert_eq!(space.used, 0); // Should saturate to 0, not overflow
	}

	#[test]
	fn test_mock_space_checker() -> io::Result<()> {
		let mock = MockSpaceChecker { available: 1000 };

		assert_eq!(mock.get_available_space(Path::new("/"))?, 1000);

		Ok(())
	}

	#[test]
	fn test_real_space_checker() -> io::Result<()> {
		let checker = RealSpaceChecker;
		let current_dir = env::current_dir()?;

		// Test that the real checker returns valid values
		let available = checker.get_available_space(&current_dir)?;

		assert!(available > 0);

		Ok(())
	}
}
