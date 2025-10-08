use std::fs;
use std::io;
use std::path::Path;

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
/// On Unix systems, this uses the statvfs system call.
/// On Windows, this uses the GetDiskFreeSpaceEx Windows API call.
pub fn get_disk_space(path: &Path) -> io::Result<DiskSpace> {
	// Ensure the path exists
	if !path.exists() {
		return Err(io::Error::new(
			io::ErrorKind::NotFound,
			format!("Path does not exist: {:?}", path),
		));
	}

	#[cfg(unix)]
	{
		get_disk_space_unix(path)
	}

	#[cfg(windows)]
	{
		use std::ffi::OsStr;
		use std::os::windows::ffi::OsStrExt;
		use std::ptr;
		use winapi::um::fileapi::GetDiskFreeSpaceExW;

		// Get the root path of the drive
		let root_path = path.ancestors().last().ok_or_else(|| {
			io::Error::new(io::ErrorKind::InvalidInput, "Cannot determine drive root")
		})?;

		// Convert path to wide string for Windows API
		let wide_path: Vec<u16> = OsStr::new(root_path).encode_wide().chain(Some(0)).collect();

		let mut free_bytes_available: u64 = 0;
		let mut total_bytes: u64 = 0;
		let mut total_free_bytes: u64 = 0;

		let result = unsafe {
			GetDiskFreeSpaceExW(
				wide_path.as_ptr(),
				&mut free_bytes_available as *mut _,
				&mut total_bytes as *mut _,
				&mut total_free_bytes as *mut _,
			)
		};

		if result == 0 {
			return Err(io::Error::last_os_error());
		}

		Ok(DiskSpace::new(total_bytes, free_bytes_available))
	}
}

/// Unix-specific implementation using the statvfs system call
///
/// This function uses `unsafe` because it must interface with C library functions
/// from libc to query the filesystem. Rust has no built-in cross-platform way to
/// get disk space information, so we must use platform-specific system calls.
///
/// The unsafe operations are:
/// 1. `mem::zeroed()` - Creates a zeroed C struct (libc::statvfs). This is safe
///    because the struct is a Plain Old Data (POD) type with no invariants.
/// 2. `libc::fstatvfs()` - Calls the POSIX fstatvfs(2) system call. This is safe
///    when given a valid file descriptor and a properly initialized struct pointer.
///
/// Safety invariants maintained:
/// - We pass a valid file descriptor from an open File handle
/// - The statvfs struct is properly zero-initialized before passing to fstatvfs
/// - We check the return value (-1) to detect errors
/// - The File handle keeps the fd valid for the duration of the call
#[cfg(unix)]
fn get_disk_space_unix(path: &Path) -> io::Result<DiskSpace> {
	use std::mem;
	use std::os::unix::io::AsRawFd;

	// Get metadata to determine the device
	let _metadata = fs::metadata(path)?;

	// Open the directory (or parent directory if it's a file)
	// statvfs needs a file descriptor, so we need an actual directory
	let dir_path = if path.is_dir() {
		path.to_path_buf()
	} else {
		path.parent()
			.ok_or_else(|| {
				io::Error::new(io::ErrorKind::InvalidInput, "Cannot get parent directory")
			})?
			.to_path_buf()
	};

	let dir = fs::File::open(dir_path)?;
	let fd = dir.as_raw_fd();

	// SAFETY: We need to call the C library function fstatvfs to get filesystem statistics.
	// There is no safe Rust alternative for querying disk space on Unix systems.
	//
	// 1. Initialize a zeroed statvfs struct - this is safe because libc::statvfs is a
	//    C struct (POD type) with no Rust-level invariants that need upholding.
	let mut stat: libc::statvfs = unsafe { mem::zeroed() };

	// 2. Call fstatvfs with our file descriptor and struct pointer.
	//    This is safe because:
	//    - `fd` is a valid file descriptor (we just opened it above)
	//    - `stat` is a valid, properly aligned pointer to a statvfs struct
	//    - The File handle (`dir`) keeps the fd alive during this call
	let result = unsafe { libc::fstatvfs(fd, &mut stat) };

	// Check for errors (fstatvfs returns -1 on error)
	if result == -1 {
		return Err(io::Error::last_os_error());
	}

	// Extract filesystem statistics from the C struct
	// These fields contain block sizes and counts for total/available space
	let block_size = stat.f_frsize;
	let total_blocks = stat.f_blocks;
	let available_blocks = stat.f_bavail;

	// Calculate total and available bytes
	let total = total_blocks * block_size;
	let available = available_blocks * block_size;

	Ok(DiskSpace::new(total, available))
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
		let result = get_disk_space(Path::new("/nonexistent/path/that/should/not/exist"));
		assert!(result.is_err(), "Should fail for nonexistent path");
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
