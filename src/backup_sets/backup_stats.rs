use bytesize::ByteSize;
use chrono::{DateTime, Utc};
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
	start_timestamp: DateTime<Utc>,
	files_hardlinked: AtomicUsize,
	files_copied: AtomicUsize,
	bytes_hardlinked: AtomicU64,
	bytes_copied: AtomicU64,
	bytes_source_read: AtomicU64,
	bytes_target_read: AtomicU64,
	bytes_target_written: AtomicU64,
	bytes_hashed: AtomicU64,
	backup_root: PathBuf,
	session_id: String,
}

impl BackupStats {
	/// Create a new BackupStats instance for tracking backup statistics
	pub fn new(backup_root: &Path, session_id: &str) -> Self {
		BackupStats {
			inner: Arc::new(BackupStatsInner {
				start_time: Instant::now(),
				start_timestamp: Utc::now(),
				files_hardlinked: AtomicUsize::new(0),
				files_copied: AtomicUsize::new(0),
				bytes_hardlinked: AtomicU64::new(0),
				bytes_copied: AtomicU64::new(0),
				bytes_source_read: AtomicU64::new(0),
				bytes_target_read: AtomicU64::new(0),
				bytes_target_written: AtomicU64::new(0),
				bytes_hashed: AtomicU64::new(0),
				backup_root: backup_root.to_path_buf(),
				session_id: session_id.to_string(),
			}),
		}
	}

	/// Track bytes read from source file
	pub fn add_source_read(&self, bytes: u64) {
		self.inner
			.bytes_source_read
			.fetch_add(bytes, Ordering::Relaxed);
	}

	/// Track bytes read from target (for verification during hardlinking)
	#[allow(dead_code)]
	pub fn add_target_read(&self, bytes: u64) {
		self.inner
			.bytes_target_read
			.fetch_add(bytes, Ordering::Relaxed);
	}

	/// Track bytes written to target
	pub fn add_target_written(&self, bytes: u64) {
		self.inner
			.bytes_target_written
			.fetch_add(bytes, Ordering::Relaxed);
	}

	/// Track bytes that were hashed
	pub fn add_hashed(&self, bytes: u64) {
		self.inner.bytes_hashed.fetch_add(bytes, Ordering::Relaxed);
	}

	/// Record that a file was hardlinked (no new data written)
	pub fn add_file_hardlinked(&self, file_size: u64) {
		self.inner.files_hardlinked.fetch_add(1, Ordering::Relaxed);
		self.inner
			.bytes_hardlinked
			.fetch_add(file_size, Ordering::Relaxed);
	}

	/// Record that a file was copied (new data written)
	pub fn add_file_copied(&self, file_size: u64) {
		self.inner.files_copied.fetch_add(1, Ordering::Relaxed);
		self.inner
			.bytes_copied
			.fetch_add(file_size, Ordering::Relaxed);
	}

	/// Get the current elapsed time since backup started
	pub fn elapsed(&self) -> Duration {
		self.inner.start_time.elapsed()
	}

	/// Format duration as HH:MM:SS.mmm
	fn format_duration(duration: Duration) -> String {
		let total_millis = duration.as_millis();
		let hours = total_millis / 3_600_000;
		let minutes = (total_millis % 3_600_000) / 60_000;
		let seconds = (total_millis % 60_000) / 1_000;
		let millis = total_millis % 1_000;
		format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
	}

	/// Save the statistics to a formatted text file in the backup root directory
	pub fn save(&self) -> io::Result<()> {
		let stats_path = self.inner.backup_root.join(STATS_FILENAME);
		let mut file = File::create(&stats_path)?;

		let elapsed = self.elapsed();
		let end_timestamp = Utc::now();

		// Load all values
		let files_hardlinked = self.inner.files_hardlinked.load(Ordering::Relaxed);
		let files_copied = self.inner.files_copied.load(Ordering::Relaxed);
		let files_total = files_hardlinked + files_copied;

		let bytes_hardlinked = self.inner.bytes_hardlinked.load(Ordering::Relaxed);
		let bytes_copied = self.inner.bytes_copied.load(Ordering::Relaxed);
		let bytes_total = bytes_hardlinked + bytes_copied;

		let bytes_source_read = self.inner.bytes_source_read.load(Ordering::Relaxed);
		let bytes_target_read = self.inner.bytes_target_read.load(Ordering::Relaxed);
		let bytes_target_written = self.inner.bytes_target_written.load(Ordering::Relaxed);
		let bytes_hashed = self.inner.bytes_hashed.load(Ordering::Relaxed);

		// Write the formatted stats file
		writeln!(file, "Backup Summary")?;
		writeln!(file, "==============")?;
		writeln!(
			file,
			"Program: disk-hog-backup {}",
			env!("CARGO_PKG_VERSION")
		)?;
		writeln!(file, "Time format: HH:MM:SS.mmm")?;
		writeln!(file, "Sizes: bytes (with human-readable shown)")?;
		writeln!(file)?;
		writeln!(file, "Session ID: {}", self.inner.session_id)?;
		writeln!(file)?;
		writeln!(file, "Time:")?;
		writeln!(
			file,
			"  Started:  {}",
			self.inner
				.start_timestamp
				.format("%Y-%m-%d %H:%M:%S%.3f UTC")
		)?;
		writeln!(
			file,
			"  Finished: {}",
			end_timestamp.format("%Y-%m-%d %H:%M:%S%.3f UTC")
		)?;
		writeln!(file, "  Duration: {}", Self::format_duration(elapsed))?;
		writeln!(file)?;
		writeln!(file, "Backup Set Stats:")?;
		writeln!(
			file,
			"  Hardlinked: {} files, {}",
			files_hardlinked,
			ByteSize(bytes_hardlinked)
		)?;
		writeln!(
			file,
			"  Copied:     {} files, {}",
			files_copied,
			ByteSize(bytes_copied)
		)?;
		writeln!(
			file,
			"  Total:      {} files, {}",
			files_total,
			ByteSize(bytes_total)
		)?;
		writeln!(file)?;
		writeln!(file, "I/O:")?;
		writeln!(
			file,
			"  Source Read: {} ({})",
			bytes_source_read,
			ByteSize(bytes_source_read)
		)?;
		writeln!(
			file,
			"  Target Read: {} ({})",
			bytes_target_read,
			ByteSize(bytes_target_read)
		)?;
		writeln!(
			file,
			"  Target Written: {} ({})",
			bytes_target_written,
			ByteSize(bytes_target_written)
		)?;
		writeln!(
			file,
			"  Hashing: {} ({})",
			bytes_hashed,
			ByteSize(bytes_hashed)
		)?;

		println!("\nBackup Statistics saved to: {}", stats_path.display());
		self.print_summary();

		Ok(())
	}

	/// Print a summary of the statistics to console
	pub fn print_summary(&self) {
		let elapsed = self.elapsed();
		let end_timestamp = Utc::now();

		// Load all values
		let files_hardlinked = self.inner.files_hardlinked.load(Ordering::Relaxed);
		let files_copied = self.inner.files_copied.load(Ordering::Relaxed);
		let files_total = files_hardlinked + files_copied;

		let bytes_hardlinked = self.inner.bytes_hardlinked.load(Ordering::Relaxed);
		let bytes_copied = self.inner.bytes_copied.load(Ordering::Relaxed);
		let bytes_total = bytes_hardlinked + bytes_copied;

		let bytes_source_read = self.inner.bytes_source_read.load(Ordering::Relaxed);
		let bytes_target_read = self.inner.bytes_target_read.load(Ordering::Relaxed);
		let bytes_target_written = self.inner.bytes_target_written.load(Ordering::Relaxed);
		let bytes_hashed = self.inner.bytes_hashed.load(Ordering::Relaxed);

		println!("\nBackup Summary");
		println!("==============");
		println!("Program: disk-hog-backup {}", env!("CARGO_PKG_VERSION"));
		println!("Time format: HH:MM:SS.mmm");
		println!("Sizes: bytes (with human-readable shown)");
		println!();
		println!("Session ID: {}", self.inner.session_id);
		println!();
		println!("Time:");
		println!(
			"  Started:  {}",
			self.inner
				.start_timestamp
				.format("%Y-%m-%d %H:%M:%S%.3f UTC")
		);
		println!(
			"  Finished: {}",
			end_timestamp.format("%Y-%m-%d %H:%M:%S%.3f UTC")
		);
		println!("  Duration: {}", Self::format_duration(elapsed));
		println!();
		println!("Backup Set Stats:");
		println!(
			"  Hardlinked: {} files, {}",
			files_hardlinked,
			ByteSize(bytes_hardlinked)
		);
		println!(
			"  Copied:     {} files, {}",
			files_copied,
			ByteSize(bytes_copied)
		);
		println!(
			"  Total:      {} files, {}",
			files_total,
			ByteSize(bytes_total)
		);
		println!();
		println!("I/O:");
		println!(
			"  Source Read: {} ({})",
			bytes_source_read,
			ByteSize(bytes_source_read)
		);
		println!(
			"  Target Read: {} ({})",
			bytes_target_read,
			ByteSize(bytes_target_read)
		);
		println!(
			"  Target Written: {} ({})",
			bytes_target_written,
			ByteSize(bytes_target_written)
		);
		println!("  Hashing: {} ({})", bytes_hashed, ByteSize(bytes_hashed));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::thread;
	use tempfile::tempdir;

	#[test]
	fn test_backup_stats_creation() {
		let temp_dir = tempdir().unwrap();
		let stats = BackupStats::new(temp_dir.path(), "test-session-id");

		assert_eq!(stats.inner.files_hardlinked.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.files_copied.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_hardlinked.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_copied.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_source_read.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_target_read.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_target_written.load(Ordering::Relaxed), 0);
		assert_eq!(stats.inner.bytes_hashed.load(Ordering::Relaxed), 0);
	}

	#[test]
	fn test_adding_file_statistics() {
		let temp_dir = tempdir().unwrap();
		let stats = BackupStats::new(temp_dir.path(), "test-session-id");

		// Add some I/O operations
		stats.add_source_read(1024 * 1024); // Read 1 MB from source
		stats.add_hashed(1024 * 1024); // Hash 1 MB
		stats.add_file_hardlinked(1024 * 1024); // 1 MB hardlinked

		stats.add_source_read(2 * 1024 * 1024); // Read 2 MB from source
		stats.add_hashed(2 * 1024 * 1024); // Hash 2 MB
		stats.add_target_written(2 * 1024 * 1024); // Write 2 MB to target
		stats.add_file_copied(2 * 1024 * 1024); // 2 MB copied

		assert_eq!(stats.inner.files_hardlinked.load(Ordering::Relaxed), 1);
		assert_eq!(stats.inner.files_copied.load(Ordering::Relaxed), 1);
		assert_eq!(
			stats.inner.bytes_source_read.load(Ordering::Relaxed),
			3 * 1024 * 1024
		);
		assert_eq!(
			stats.inner.bytes_hardlinked.load(Ordering::Relaxed),
			1024 * 1024
		);
		assert_eq!(
			stats.inner.bytes_copied.load(Ordering::Relaxed),
			2 * 1024 * 1024
		);
		assert_eq!(
			stats.inner.bytes_target_written.load(Ordering::Relaxed),
			2 * 1024 * 1024
		);
		assert_eq!(
			stats.inner.bytes_hashed.load(Ordering::Relaxed),
			3 * 1024 * 1024
		);
	}

	#[test]
	fn test_thread_safety() {
		let temp_dir = tempdir().unwrap();
		let stats = BackupStats::new(temp_dir.path(), "test-session-id");

		let threads: Vec<_> = (0..10)
			.map(|_| {
				let stats_clone = stats.clone();
				thread::spawn(move || {
					for _ in 0..100 {
						stats_clone.add_source_read(1024);
						stats_clone.add_hashed(1024);
						stats_clone.add_file_hardlinked(512);
						stats_clone.add_file_copied(512);
					}
				})
			})
			.collect();

		for t in threads {
			t.join().unwrap();
		}

		assert_eq!(stats.inner.files_hardlinked.load(Ordering::Relaxed), 1000);
		assert_eq!(stats.inner.files_copied.load(Ordering::Relaxed), 1000);
		assert_eq!(
			stats.inner.bytes_source_read.load(Ordering::Relaxed),
			1000 * 1024
		);
	}

	#[test]
	fn test_save_stats_format() {
		let temp_dir = tempdir().unwrap();
		let backup_path = temp_dir.path().join("backup");
		std::fs::create_dir(&backup_path).unwrap();
		let stats = BackupStats::new(&backup_path, "dhb-set-20250929-131320");

		// Add some data
		stats.add_source_read(7 * 1024 * 1024 * 1024); // 7 GB read
		stats.add_hashed(7 * 1024 * 1024 * 1024); // 7 GB hashed
		stats.add_file_hardlinked(3 * 1024 * 1024 * 1024); // 3 GB hardlinked
		stats.add_file_copied(2 * 1024 * 1024 * 1024); // 2 GB copied
		stats.add_target_written(2 * 1024 * 1024 * 1024); // 2 GB written

		// Sleep briefly to get non-zero timing
		thread::sleep(Duration::from_millis(10));

		// Save stats
		stats.save().unwrap();

		// Read and verify file contents
		let stats_path = backup_path.join(STATS_FILENAME);
		let contents = std::fs::read_to_string(stats_path).unwrap();

		// Verify key sections exist
		assert!(contents.contains("Backup Summary"));
		assert!(contents.contains("Session ID: dhb-set-20250929-131320"));
		assert!(contents.contains("Time:"));
		assert!(contents.contains("Backup Set Stats:"));
		assert!(contents.contains("I/O:"));

		// Verify data is present
		assert!(contents.contains("Hardlinked: 1 files"));
		assert!(contents.contains("Copied:     1 files"));
		assert!(contents.contains("Total:      2 files"));
	}

	#[test]
	fn test_format_duration() {
		assert_eq!(
			BackupStats::format_duration(Duration::from_millis(0)),
			"00:00:00.000"
		);
		assert_eq!(
			BackupStats::format_duration(Duration::from_millis(123)),
			"00:00:00.123"
		);
		assert_eq!(
			BackupStats::format_duration(Duration::from_millis(65_123)),
			"00:01:05.123"
		);
		assert_eq!(
			BackupStats::format_duration(Duration::from_millis(3_665_123)),
			"01:01:05.123"
		);
	}
}
