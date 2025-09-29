use bytesize::ByteSize;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const STATS_FILENAME: &str = "disk-hog-backup-stats.txt";

/// Statistics for a backup operation, designed to be thread-safe for parallel processing
#[derive(Clone)]
pub struct BackupStats {
	inner: Arc<BackupStatsInner>,
}

struct BackupStatsInner {
	start_time: Instant,
	files_processed: AtomicUsize,
	files_hardlinked: AtomicUsize,
	files_copied: AtomicUsize,
	bytes_processed: AtomicU64,
	bytes_saved: AtomicU64,
	bytes_written: AtomicU64,
	backup_root: PathBuf,
}

impl BackupStats {
	/// Create a new BackupStats instance for tracking backup statistics
	pub fn new(backup_root: &Path) -> Self {
		BackupStats {
			inner: Arc::new(BackupStatsInner {
				start_time: Instant::now(),
				files_processed: AtomicUsize::new(0),
				files_hardlinked: AtomicUsize::new(0),
				files_copied: AtomicUsize::new(0),
				bytes_processed: AtomicU64::new(0),
				bytes_saved: AtomicU64::new(0),
				bytes_written: AtomicU64::new(0),
				backup_root: backup_root.to_path_buf(),
			}),
		}
	}

	/// Record a file that was processed
	pub fn add_file_processed(&self, file_size: u64) {
		self.inner.files_processed.fetch_add(1, Ordering::Relaxed);
		self.inner
			.bytes_processed
			.fetch_add(file_size, Ordering::Relaxed);
	}

	/// Record a file that was hardlinked (space saved)
	pub fn add_file_hardlinked(&self, file_size: u64) {
		self.inner.files_hardlinked.fetch_add(1, Ordering::Relaxed);
		self.inner
			.bytes_saved
			.fetch_add(file_size, Ordering::Relaxed);
	}

	/// Record a file that was copied (new data written)
	pub fn add_file_copied(&self, file_size: u64) {
		self.inner.files_copied.fetch_add(1, Ordering::Relaxed);
		self.inner
			.bytes_written
			.fetch_add(file_size, Ordering::Relaxed);
	}

	/// Get the current elapsed time since backup started
	pub fn elapsed(&self) -> Duration {
		self.inner.start_time.elapsed()
	}

	/// Save the statistics to a tab-separated text file in the backup root directory
	pub fn save(&self) -> io::Result<()> {
		let stats_path = self.inner.backup_root.join(STATS_FILENAME);
		let mut file = File::create(&stats_path)?;

		let elapsed = self.elapsed();
		let processing_time_secs = elapsed.as_secs_f64();

		let files_processed = self.inner.files_processed.load(Ordering::Relaxed);
		let files_hardlinked = self.inner.files_hardlinked.load(Ordering::Relaxed);
		let files_copied = self.inner.files_copied.load(Ordering::Relaxed);
		let bytes_processed = self.inner.bytes_processed.load(Ordering::Relaxed);
		let bytes_saved = self.inner.bytes_saved.load(Ordering::Relaxed);
		let bytes_written = self.inner.bytes_written.load(Ordering::Relaxed);

		let processing_speed_bps = if processing_time_secs > 0.0 {
			bytes_processed as f64 / processing_time_secs
		} else {
			0.0
		};

		// Write header
		writeln!(file, "stat_name\tbytes\thuman_readable")?;

		// Write timing stats
		writeln!(
			file,
			"processing_time_seconds\t{:.1}\t{:.1} seconds",
			processing_time_secs, processing_time_secs
		)?;
		writeln!(
			file,
			"processing_time_minutes\t{:.2}\t{:.2} minutes",
			processing_time_secs / 60.0,
			processing_time_secs / 60.0
		)?;

		// Write file counts
		writeln!(
			file,
			"files_processed\t{}\t{} files",
			files_processed, files_processed
		)?;
		writeln!(
			file,
			"files_hardlinked\t{}\t{} files",
			files_hardlinked, files_hardlinked
		)?;
		writeln!(
			file,
			"files_copied\t{}\t{} files",
			files_copied, files_copied
		)?;

		// Write byte stats with human-readable sizes
		writeln!(
			file,
			"bytes_processed\t{}\t{}",
			bytes_processed,
			ByteSize(bytes_processed)
		)?;
		writeln!(
			file,
			"bytes_saved_via_hardlink\t{}\t{}",
			bytes_saved,
			ByteSize(bytes_saved)
		)?;
		writeln!(
			file,
			"bytes_written_new_data\t{}\t{}",
			bytes_written,
			ByteSize(bytes_written)
		)?;

		// Write speed
		writeln!(
			file,
			"processing_speed_bytes_per_sec\t{:.0}\t{}/s",
			processing_speed_bps,
			ByteSize(processing_speed_bps as u64)
		)?;

		println!("\nBackup Statistics saved to: {}", stats_path.display());
		self.print_summary();

		Ok(())
	}

	/// Print a summary of the statistics
	pub fn print_summary(&self) {
		let elapsed = self.elapsed();
		let processing_time_secs = elapsed.as_secs_f64();

		let files_processed = self.inner.files_processed.load(Ordering::Relaxed);
		let files_hardlinked = self.inner.files_hardlinked.load(Ordering::Relaxed);
		let files_copied = self.inner.files_copied.load(Ordering::Relaxed);
		let bytes_processed = self.inner.bytes_processed.load(Ordering::Relaxed);
		let bytes_saved = self.inner.bytes_saved.load(Ordering::Relaxed);
		let bytes_written = self.inner.bytes_written.load(Ordering::Relaxed);

		let processing_speed_bps = if processing_time_secs > 0.0 {
			bytes_processed as f64 / processing_time_secs
		} else {
			0.0
		};

		println!("\n==== Backup Statistics Summary ====");
		println!(
			"Processing Time: {:.1} seconds ({:.1} minutes)",
			processing_time_secs,
			processing_time_secs / 60.0
		);
		println!("Files Processed: {}", files_processed);
		println!("  - Hardlinked: {} (space saved)", files_hardlinked);
		println!("  - Copied: {} (new data)", files_copied);
		println!();
		println!("Data Processed: {}", ByteSize(bytes_processed));
		println!("Space Saved: {} (via hardlinking)", ByteSize(bytes_saved));
		println!("New Data Written: {}", ByteSize(bytes_written));
		println!(
			"Processing Speed: {}/s",
			ByteSize(processing_speed_bps as u64)
		);
		println!("===================================");
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::thread;
	use std::time::Duration;
	use tempfile::tempdir;

	#[test]
	fn test_backup_stats_creation() {
		let temp_dir = tempdir().unwrap();
		let stats = BackupStats::new(temp_dir.path());

		assert_eq!(stats.inner.files_processed.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.files_hardlinked.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.files_copied.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_processed.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_saved.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_written.load(Ordering::Relaxed), 0);
	}

	#[test]
	fn test_adding_file_statistics() {
		let temp_dir = tempdir().unwrap();
		let stats = BackupStats::new(temp_dir.path());

		// Add some file operations
		stats.add_file_processed(1024 * 1024); // 1 MB
		stats.add_file_hardlinked(1024 * 1024); // 1 MB hardlinked

		stats.add_file_processed(2 * 1024 * 1024); // 2 MB
		stats.add_file_copied(2 * 1024 * 1024); // 2 MB copied

		assert_eq!(stats.inner.files_processed.load(Ordering::Relaxed), 2);
		assert_eq!(stats.inner.files_hardlinked.load(Ordering::Relaxed), 1);
		assert_eq!(stats.inner.files_copied.load(Ordering::Relaxed), 1);
		assert_eq!(
			stats.inner.bytes_processed.load(Ordering::Relaxed),
			3 * 1024 * 1024
		);
		assert_eq!(stats.inner.bytes_saved.load(Ordering::Relaxed), 1024 * 1024);
		assert_eq!(
			stats.inner.bytes_written.load(Ordering::Relaxed),
			2 * 1024 * 1024
		);
	}

	#[test]
	fn test_thread_safety() {
		let temp_dir = tempdir().unwrap();
		let stats = BackupStats::new(temp_dir.path());

		let threads: Vec<_> = (0..10)
			.map(|_| {
				let stats_clone = stats.clone();
				thread::spawn(move || {
					for _ in 0..100 {
						stats_clone.add_file_processed(1024);
						stats_clone.add_file_hardlinked(512);
						stats_clone.add_file_copied(512);
					}
				})
			})
			.collect();

		for t in threads {
			t.join().unwrap();
		}

		assert_eq!(stats.inner.files_processed.load(Ordering::Relaxed), 1000);
		assert_eq!(stats.inner.files_hardlinked.load(Ordering::Relaxed), 1000);
		assert_eq!(stats.inner.files_copied.load(Ordering::Relaxed), 1000);
	}

	#[test]
	fn test_save_stats_format() {
		let temp_dir = tempdir().unwrap();
		let stats = BackupStats::new(temp_dir.path());

		// Add some data
		stats.add_file_processed(5 * 1024 * 1024 * 1024); // 5 GB
		stats.add_file_hardlinked(3 * 1024 * 1024 * 1024); // 3 GB
		stats.add_file_copied(2 * 1024 * 1024 * 1024); // 2 GB

		// Save stats
		stats.save().unwrap();

		// Read and verify file contents
		let stats_path = temp_dir.path().join(STATS_FILENAME);
		let contents = std::fs::read_to_string(stats_path).unwrap();

		// Verify header exists
		assert!(contents.contains("stat_name\tbytes\thuman_readable"));

		// Verify some specific entries exist
		assert!(contents.contains("files_processed\t1\t1 files"));
		assert!(contents.contains("files_hardlinked\t1\t1 files"));
		assert!(contents.contains("files_copied\t1\t1 files"));
		assert!(contents.contains("bytes_processed\t5368709120\t5.4 GB")); // ByteSize uses decimal units
		assert!(contents.contains("bytes_saved_via_hardlink\t3221225472\t3.2 GB"));
		assert!(contents.contains("bytes_written_new_data\t2147483648\t2.1 GB"));
	}

	#[test]
	fn test_timing_and_speed() {
		let temp_dir = tempdir().unwrap();
		let stats = BackupStats::new(temp_dir.path());

		// Add 100 MB of data
		stats.add_file_processed(100 * 1024 * 1024);

		// Sleep briefly to ensure elapsed time is non-zero
		thread::sleep(Duration::from_millis(10));

		let elapsed = stats.elapsed();
		assert!(elapsed.as_secs_f64() > 0.0);
	}

	#[test]
	fn test_bytesize_formatting() {
		// Test that ByteSize gives us nice human-readable output
		// Note: ByteSize uses decimal units by default (1000-based, not 1024-based)
		println!("1024 bytes: {}", ByteSize(1024));
		println!("1MB (1024*1024): {}", ByteSize(1024 * 1024));
		println!("1GB (1024*1024*1024): {}", ByteSize(1024 * 1024 * 1024));
		println!("2GB: {}", ByteSize(2 * 1024 * 1024 * 1024));
		println!("5GB: {}", ByteSize(5 * 1024 * 1024 * 1024));

		// Test actual values from our tests
		assert_eq!(format!("{}", ByteSize(1024)), "1.0 KB");
		// For binary units (1024-based), the formatting will be different from decimal
		let one_mib = 1024 * 1024;
		let one_gib = 1024 * 1024 * 1024;

		// Just verify they format to something reasonable
		let mb_str = format!("{}", ByteSize(one_mib));
		assert!(mb_str.contains("MB") || mb_str.contains("KB"));

		let gb_str = format!("{}", ByteSize(one_gib));
		assert!(gb_str.contains("GB") || gb_str.contains("MB"));
	}
}
