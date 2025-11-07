use crate::disk_space::DiskSpace;
use bytesize::ByteSize;
use chrono::{DateTime, Utc};
use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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
	files_new: AtomicUsize,           // new file, no previous backup
	files_size_changed: AtomicUsize,  // size changed, no previous file to compare
	files_mtime_changed: AtomicUsize, // mtime changed, had to check hash
	files_hash_changed: AtomicUsize,  // hash changed, had to copy
	bytes_source_read: AtomicU64,
	bytes_target_read: AtomicU64,
	bytes_target_written: AtomicU64,
	bytes_hashed: AtomicU64,
	backup_root: PathBuf,
	session_id: String,

	// Pipeline timing (nanoseconds) - cumulative across all files
	reader_io_nanos: AtomicU64,
	reader_send_writer_nanos: AtomicU64,
	reader_send_hasher_nanos: AtomicU64,
	hasher_recv_nanos: AtomicU64,
	hasher_hash_nanos: AtomicU64,
	writer_recv_nanos: AtomicU64,
	writer_io_nanos: AtomicU64,
	memory_throttle_nanos: AtomicU64,
	memory_throttle_count: AtomicU64,

	// Queue depth sampling
	writer_queue_depth_samples: AtomicU64,
	writer_queue_depth_sum: AtomicU64,
	writer_queue_depth_max: AtomicU64,
	hasher_queue_depth_samples: AtomicU64,
	hasher_queue_depth_sum: AtomicU64,
	hasher_queue_depth_max: AtomicU64,

	// Terminal detection for interactive progress display
	is_terminal: bool,

	// Total size of source (for progress percentage)
	total_bytes: u64,

	// Time taken to calculate total size
	size_calc_duration: Duration,

	// Disk space tracking (mutable, protected by Mutex)
	disk_space: Mutex<DiskSpaceInfo>,

	// Auto-deleted backup sets (mutable, protected by Mutex)
	deleted_sets: Mutex<Vec<String>>,
}

struct DiskSpaceInfo {
	initial: Option<DiskSpace>,
	final_space: Option<DiskSpace>,
	md5_store_size: Option<u64>,
}

impl BackupStats {
	/// Create a new BackupStats instance for tracking backup statistics
	pub fn new(
		backup_root: &Path,
		session_id: &str,
		total_bytes: u64,
		size_calc_duration: Duration,
		initial_disk_space: Option<DiskSpace>,
	) -> Self {
		BackupStats {
			inner: Arc::new(BackupStatsInner {
				start_time: Instant::now(),
				start_timestamp: Utc::now(),
				files_hardlinked: AtomicUsize::new(0),
				files_copied: AtomicUsize::new(0),
				bytes_hardlinked: AtomicU64::new(0),
				bytes_copied: AtomicU64::new(0),
				files_new: AtomicUsize::new(0),
				files_size_changed: AtomicUsize::new(0),
				files_mtime_changed: AtomicUsize::new(0),
				files_hash_changed: AtomicUsize::new(0),
				bytes_source_read: AtomicU64::new(0),
				bytes_target_read: AtomicU64::new(0),
				bytes_target_written: AtomicU64::new(0),
				bytes_hashed: AtomicU64::new(0),
				backup_root: backup_root.to_path_buf(),
				session_id: session_id.to_string(),

				// Pipeline timing
				reader_io_nanos: AtomicU64::new(0),
				reader_send_writer_nanos: AtomicU64::new(0),
				reader_send_hasher_nanos: AtomicU64::new(0),
				hasher_recv_nanos: AtomicU64::new(0),
				hasher_hash_nanos: AtomicU64::new(0),
				writer_recv_nanos: AtomicU64::new(0),
				writer_io_nanos: AtomicU64::new(0),
				memory_throttle_nanos: AtomicU64::new(0),
				memory_throttle_count: AtomicU64::new(0),

				// Queue depth sampling
				writer_queue_depth_samples: AtomicU64::new(0),
				writer_queue_depth_sum: AtomicU64::new(0),
				writer_queue_depth_max: AtomicU64::new(0),
				hasher_queue_depth_samples: AtomicU64::new(0),
				hasher_queue_depth_sum: AtomicU64::new(0),
				hasher_queue_depth_max: AtomicU64::new(0),

				// Check if stderr is a terminal for interactive progress
				is_terminal: std::io::stderr().is_terminal(),

				total_bytes,
				size_calc_duration,
				disk_space: Mutex::new(DiskSpaceInfo {
					initial: initial_disk_space,
					final_space: None,
					md5_store_size: None,
				}),
				deleted_sets: Mutex::new(Vec::new()),
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

	/// Record that size changed
	pub fn add_file_new(&self) {
		self.inner.files_new.fetch_add(1, Ordering::Relaxed);
	}

	pub fn add_file_size_changed(&self) {
		self.inner
			.files_size_changed
			.fetch_add(1, Ordering::Relaxed);
	}

	/// Record that mtime changed
	pub fn add_file_mtime_changed(&self) {
		self.inner
			.files_mtime_changed
			.fetch_add(1, Ordering::Relaxed);
	}

	/// Record that hash changed
	pub fn add_file_hash_changed(&self) {
		self.inner
			.files_hash_changed
			.fetch_add(1, Ordering::Relaxed);
	}

	/// Get the current elapsed time since backup started
	pub fn elapsed(&self) -> Duration {
		self.inner.start_time.elapsed()
	}

	// Pipeline telemetry methods

	/// Record time spent in reader I/O
	pub fn add_reader_io_time(&self, nanos: u64) {
		self.inner
			.reader_io_nanos
			.fetch_add(nanos, Ordering::Relaxed);
	}

	/// Record time spent blocked on writer channel send
	pub fn add_reader_send_writer_time(&self, nanos: u64) {
		self.inner
			.reader_send_writer_nanos
			.fetch_add(nanos, Ordering::Relaxed);
	}

	/// Record time spent blocked on hasher channel send
	pub fn add_reader_send_hasher_time(&self, nanos: u64) {
		self.inner
			.reader_send_hasher_nanos
			.fetch_add(nanos, Ordering::Relaxed);
	}

	/// Record time spent in hasher receive
	pub fn add_hasher_recv_time(&self, nanos: u64) {
		self.inner
			.hasher_recv_nanos
			.fetch_add(nanos, Ordering::Relaxed);
	}

	/// Record time spent in MD5 hashing
	pub fn add_hasher_hash_time(&self, nanos: u64) {
		self.inner
			.hasher_hash_nanos
			.fetch_add(nanos, Ordering::Relaxed);
	}

	/// Record time spent blocked on writer channel receive
	pub fn add_writer_recv_time(&self, nanos: u64) {
		self.inner
			.writer_recv_nanos
			.fetch_add(nanos, Ordering::Relaxed);
	}

	/// Record time spent in writer I/O
	pub fn add_writer_io_time(&self, nanos: u64) {
		self.inner
			.writer_io_nanos
			.fetch_add(nanos, Ordering::Relaxed);
	}

	/// Record time spent waiting on memory throttle
	pub fn add_memory_throttle_time(&self, nanos: u64) {
		self.inner
			.memory_throttle_nanos
			.fetch_add(nanos, Ordering::Relaxed);
	}

	/// Increment memory throttle event count
	pub fn inc_memory_throttle_count(&self) {
		self.inner
			.memory_throttle_count
			.fetch_add(1, Ordering::Relaxed);
	}

	/// Sample writer queue depth
	pub fn sample_writer_queue_depth(&self, depth: u64) {
		self.inner
			.writer_queue_depth_samples
			.fetch_add(1, Ordering::Relaxed);
		self.inner
			.writer_queue_depth_sum
			.fetch_add(depth, Ordering::Relaxed);
		self.inner
			.writer_queue_depth_max
			.fetch_max(depth, Ordering::Relaxed);
	}

	/// Sample hasher queue depth
	pub fn sample_hasher_queue_depth(&self, depth: u64) {
		self.inner
			.hasher_queue_depth_samples
			.fetch_add(1, Ordering::Relaxed);
		self.inner
			.hasher_queue_depth_sum
			.fetch_add(depth, Ordering::Relaxed);
		self.inner
			.hasher_queue_depth_max
			.fetch_max(depth, Ordering::Relaxed);
	}

	/// Set the final disk space after backup completion
	pub fn set_final_disk_space(&self, disk_space: DiskSpace) {
		if let Ok(mut info) = self.inner.disk_space.lock() {
			info.final_space = Some(disk_space);
		}
	}

	/// Set the MD5 store file size
	pub fn set_md5_store_size(&self, size: u64) {
		if let Ok(mut info) = self.inner.disk_space.lock() {
			info.md5_store_size = Some(size);
		}
	}

	pub fn add_deleted_set(&self, set_name: String) {
		if let Ok(mut deleted_sets) = self.inner.deleted_sets.lock() {
			deleted_sets.push(set_name);
		}
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

	/// Format the backup summary as a vector of lines
	fn format_summary(&self) -> Vec<String> {
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

		let files_new = self.inner.files_new.load(Ordering::Relaxed);
		let files_size_changed = self.inner.files_size_changed.load(Ordering::Relaxed);
		let files_mtime_changed = self.inner.files_mtime_changed.load(Ordering::Relaxed);
		let files_hash_changed = self.inner.files_hash_changed.load(Ordering::Relaxed);

		let mut lines = vec![
			"Backup Summary".to_string(),
			"==============".to_string(),
			format!("Program: disk-hog-backup {}", env!("CARGO_PKG_VERSION")),
			"Time format: HH:MM:SS.mmm".to_string(),
			"Sizes: bytes (with human-readable shown)".to_string(),
			String::new(),
			format!("Session ID: {}", self.inner.session_id),
			String::new(),
			"Time:".to_string(),
			format!(
				"  Started:  {}",
				self.inner
					.start_timestamp
					.format("%Y-%m-%d %H:%M:%S%.3f UTC")
			),
			format!(
				"  Size Calc: {}",
				Self::format_duration(self.inner.size_calc_duration)
			),
			format!(
				"  Finished: {}",
				end_timestamp.format("%Y-%m-%d %H:%M:%S%.3f UTC")
			),
			format!("  Duration: {}", Self::format_duration(elapsed)),
			String::new(),
			"Backup Set Stats:".to_string(),
			format!("  New:              {}", files_new),
			format!("  Size changed:     {}", files_size_changed),
			format!("  Mtime changed:    {}", files_mtime_changed),
			format!("  Content changed:  {}", files_hash_changed),
			format!(
				"  Hardlinked:       {} files, {}",
				files_hardlinked,
				ByteSize(bytes_hardlinked)
			),
			format!(
				"  Copied:           {} files, {}",
				files_copied,
				ByteSize(bytes_copied)
			),
			format!(
				"  Total:            {} files, {}",
				files_total,
				ByteSize(bytes_total)
			),
			String::new(),
			"I/O:".to_string(),
			format!(
				"  Source Read: {} ({})",
				bytes_source_read,
				ByteSize(bytes_source_read)
			),
			format!(
				"  Target Read: {} ({})",
				bytes_target_read,
				ByteSize(bytes_target_read)
			),
			format!(
				"  Target Written: {} ({})",
				bytes_target_written,
				ByteSize(bytes_target_written)
			),
			format!("  Hashing: {} ({})", bytes_hashed, ByteSize(bytes_hashed)),
		];

		// Add pipeline stats
		lines.extend(self.format_pipeline_stats(elapsed));

		// Add disk space information
		lines.extend(self.format_disk_space_info());

		// Add deleted backup sets information
		if let Ok(deleted_sets) = self.inner.deleted_sets.lock()
			&& !deleted_sets.is_empty()
		{
			lines.push(String::new());
			lines.push("Auto-Deleted Backup Sets:".to_string());
			for set_name in deleted_sets.iter() {
				lines.push(format!("  {}", set_name));
			}
		}

		lines
	}

	/// Save the statistics to a formatted text file in the backup root directory
	pub fn save(&self) -> io::Result<()> {
		let stats_path = self.inner.backup_root.join(STATS_FILENAME);
		let mut file = File::create(&stats_path)?;

		for line in self.format_summary() {
			writeln!(file, "{}", line)?;
		}

		eprintln!("Backup Statistics saved to: {}", stats_path.display());

		Ok(())
	}

	/// Format pipeline performance statistics as strings
	fn format_pipeline_stats(&self, elapsed: Duration) -> Vec<String> {
		// Load pipeline timing values
		let reader_io_nanos = self.inner.reader_io_nanos.load(Ordering::Relaxed);
		let reader_send_writer_nanos = self.inner.reader_send_writer_nanos.load(Ordering::Relaxed);
		let reader_send_hasher_nanos = self.inner.reader_send_hasher_nanos.load(Ordering::Relaxed);
		let hasher_recv_nanos = self.inner.hasher_recv_nanos.load(Ordering::Relaxed);
		let hasher_hash_nanos = self.inner.hasher_hash_nanos.load(Ordering::Relaxed);
		let writer_recv_nanos = self.inner.writer_recv_nanos.load(Ordering::Relaxed);
		let writer_io_nanos = self.inner.writer_io_nanos.load(Ordering::Relaxed);
		let memory_throttle_nanos = self.inner.memory_throttle_nanos.load(Ordering::Relaxed);
		let memory_throttle_count = self.inner.memory_throttle_count.load(Ordering::Relaxed);

		// Load queue depth values
		let writer_queue_samples = self
			.inner
			.writer_queue_depth_samples
			.load(Ordering::Relaxed);
		let writer_queue_sum = self.inner.writer_queue_depth_sum.load(Ordering::Relaxed);
		let writer_queue_max = self.inner.writer_queue_depth_max.load(Ordering::Relaxed);
		let hasher_queue_samples = self
			.inner
			.hasher_queue_depth_samples
			.load(Ordering::Relaxed);
		let hasher_queue_sum = self.inner.hasher_queue_depth_sum.load(Ordering::Relaxed);
		let hasher_queue_max = self.inner.hasher_queue_depth_max.load(Ordering::Relaxed);

		// Skip if no pipeline activity
		if reader_io_nanos == 0 {
			return vec![];
		}

		let total_elapsed_nanos = elapsed.as_nanos() as u64;
		let mut lines = vec![
			String::new(),
			"Pipeline Performance:".to_string(),
			String::new(),
			// Reader thread
			"Reader Thread:".to_string(),
			Self::format_time_stat("  I/O", reader_io_nanos, total_elapsed_nanos),
			Self::format_time_stat(
				"  Send->Writer",
				reader_send_writer_nanos,
				total_elapsed_nanos,
			),
			Self::format_time_stat(
				"  Send->Hasher",
				reader_send_hasher_nanos,
				total_elapsed_nanos,
			),
			Self::format_time_stat("  Throttle", memory_throttle_nanos, total_elapsed_nanos),
			String::new(),
			// Hasher thread
			"Hasher Thread:".to_string(),
			Self::format_time_stat("  Blocked (recv)", hasher_recv_nanos, total_elapsed_nanos),
			Self::format_time_stat("  Hash (MD5)", hasher_hash_nanos, total_elapsed_nanos),
			String::new(),
			// Writer thread
			"Writer Thread:".to_string(),
			Self::format_time_stat("  Blocked (recv)", writer_recv_nanos, total_elapsed_nanos),
			Self::format_time_stat("  I/O", writer_io_nanos, total_elapsed_nanos),
			String::new(),
		];

		// Bottleneck analysis after threads
		let analysis = Self::format_bottleneck_analysis(
			reader_io_nanos,
			reader_send_writer_nanos + reader_send_hasher_nanos,
			hasher_recv_nanos,
			hasher_hash_nanos,
			writer_recv_nanos,
			writer_io_nanos,
			memory_throttle_nanos,
			writer_queue_samples,
			writer_queue_sum,
			hasher_queue_samples,
			hasher_queue_sum,
		);
		lines.extend(analysis);
		lines.push(String::new());

		// Queue stats
		lines.push("Queue Stats:".to_string());
		if writer_queue_samples > 0 {
			let writer_avg = writer_queue_sum as f64 / writer_queue_samples as f64;
			lines.push(format!(
				"  Writer Queue: Avg: {:.1}/32 ({:.0}%) | Peak: {}/32",
				writer_avg,
				(writer_avg / 32.0) * 100.0,
				writer_queue_max
			));
		}
		if hasher_queue_samples > 0 {
			let hasher_avg = hasher_queue_sum as f64 / hasher_queue_samples as f64;
			lines.push(format!(
				"  Hasher Queue: Avg: {:.1}/32 ({:.0}%) | Peak: {}/32",
				hasher_avg,
				(hasher_avg / 32.0) * 100.0,
				hasher_queue_max
			));
		}
		lines.push(String::new());

		// Memory throttle
		if memory_throttle_count > 0 {
			lines.push("Memory:".to_string());
			lines.push(format!("  Throttle events: {}", memory_throttle_count));
			lines.push(String::new());
		}

		lines
	}

	/// Format a single time stat with progress bar
	fn format_time_stat(label: &str, nanos: u64, total_nanos: u64) -> String {
		let seconds = nanos as f64 / 1_000_000_000.0;
		let percentage = if total_nanos > 0 {
			(nanos as f64 / total_nanos as f64) * 100.0
		} else {
			0.0
		};
		let bar_width = (percentage / 2.0) as usize; // Scale to fit nicely
		let bar = "█".repeat(bar_width);
		format!(
			"{:<20} {:>7.2}s ({:>5.1}%)  {}",
			label, seconds, percentage, bar
		)
	}

	/// Format bottleneck analysis as strings
	#[allow(clippy::too_many_arguments)]
	fn format_bottleneck_analysis(
		reader_io_nanos: u64,
		reader_blocked_nanos: u64,
		hasher_blocked_nanos: u64,
		hasher_hash_nanos: u64,
		writer_blocked_nanos: u64,
		writer_io_nanos: u64,
		memory_throttle_nanos: u64,
		writer_queue_samples: u64,
		writer_queue_sum: u64,
		hasher_queue_samples: u64,
		hasher_queue_sum: u64,
	) -> Vec<String> {
		let total = reader_io_nanos
			+ reader_blocked_nanos
			+ hasher_blocked_nanos
			+ hasher_hash_nanos
			+ writer_blocked_nanos
			+ writer_io_nanos
			+ memory_throttle_nanos;

		if total == 0 {
			return vec![];
		}

		let reader_io_pct = (reader_io_nanos as f64 / total as f64) * 100.0;
		let hasher_hash_pct = (hasher_hash_nanos as f64 / total as f64) * 100.0;
		let writer_io_pct = (writer_io_nanos as f64 / total as f64) * 100.0;
		let memory_throttle_pct = (memory_throttle_nanos as f64 / total as f64) * 100.0;

		let writer_avg = if writer_queue_samples > 0 {
			writer_queue_sum as f64 / writer_queue_samples as f64
		} else {
			0.0
		};
		let hasher_avg = if hasher_queue_samples > 0 {
			hasher_queue_sum as f64 / hasher_queue_samples as f64
		} else {
			0.0
		};

		let mut lines = vec!["Pipeline:".to_string()];

		if memory_throttle_pct > 10.0 {
			lines.push(format!(
				"  Reader throttled {:.1}% of time",
				memory_throttle_pct
			));
			lines
				.push("  Assessment: Increase GLOBAL_MAX_BUFFER or reduce concurrency".to_string());
		} else if writer_io_pct > 50.0 && writer_avg > 25.0 {
			lines.push(format!(
				"  Writer busy {:.1}%, queue {:.1}/32 full",
				writer_io_pct, writer_avg
			));
			lines.push("  Assessment: Destination disk is slow. Consider faster disk.".to_string());
		} else if reader_io_pct > 60.0 && (writer_avg < 5.0 || hasher_avg < 5.0) {
			lines.push(format!(
				"  Reader busy {:.1}%, queues near empty",
				reader_io_pct
			));
			lines.push(
				"  Assessment: Source disk is slow. Consider caching or different filesystem."
					.to_string(),
			);
		} else if hasher_hash_pct > 50.0 && hasher_avg < 5.0 {
			lines.push(format!(
				"  Hasher busy {:.1}%, queue near empty",
				hasher_hash_pct
			));
			lines.push(
				"  Assessment: MD5 computation is slow. Consider faster hash or hardware acceleration."
					.to_string(),
			);
		} else {
			lines.push(format!(
				"  Reader I/O: {:.1}%, Hasher: {:.1}%, Writer I/O: {:.1}%",
				reader_io_pct, hasher_hash_pct, writer_io_pct
			));
			lines.push("  Assessment: Pipeline appears well-tuned".to_string());
		}

		lines
	}

	/// Print a summary of the statistics to console
	pub fn print_summary(&self) {
		eprintln!();
		for line in self.format_summary() {
			eprintln!("{}", line);
		}
	}

	/// Update the progress display on stderr (overwrites same line)
	pub fn update_progress_display(&self) {
		if !self.inner.is_terminal {
			return;
		}

		let processed = self.inner.bytes_copied.load(Ordering::Relaxed)
			+ self.inner.bytes_hardlinked.load(Ordering::Relaxed);
		let processed_gb = processed as f64 / 1_000_000_000.0;
		let total_gb = self.inner.total_bytes as f64 / 1_000_000_000.0;
		let elapsed = self.elapsed();
		let elapsed_str = Self::format_duration(elapsed);

		if self.inner.total_bytes > 0 && processed > 0 {
			let percentage = (processed as f64 / self.inner.total_bytes as f64) * 100.0;

			// Calculate rate and format with appropriate units
			let elapsed_secs = elapsed.as_secs_f64();
			let rate = processed as f64 / elapsed_secs; // bytes per second
			let rate_str = if rate >= 1_000_000_000.0 {
				format!("{:.2}GB/s", rate / 1_000_000_000.0)
			} else if rate >= 1_000_000.0 {
				format!("{:.2}MB/s", rate / 1_000_000.0)
			} else if rate >= 1_000.0 {
				format!("{:.2}KB/s", rate / 1_000.0)
			} else {
				format!("{:.0}B/s", rate)
			};

			// Calculate remaining time and ETA
			let remaining_bytes = self.inner.total_bytes - processed;
			let remaining_secs = remaining_bytes as f64 / rate;
			let remaining = Duration::from_secs_f64(remaining_secs);
			let remaining_str = Self::format_duration(remaining);

			// Calculate ETA timestamp in local time
			let eta_timestamp =
				chrono::Local::now() + chrono::Duration::from_std(remaining).unwrap();
			let eta_str = eta_timestamp.format("%H:%M:%S").to_string();

			eprint!(
				"\rProgress: {:.2}GB of {:.2}GB ({:.1}%) @ {} | Time: elapsed {}, remaining {}, ETA {}",
				processed_gb, total_gb, percentage, rate_str, elapsed_str, remaining_str, eta_str
			);
		} else if self.inner.total_bytes > 0 {
			let percentage = (processed as f64 / self.inner.total_bytes as f64) * 100.0;
			eprint!(
				"\rProgress: {:.2}GB of {:.2}GB ({:.1}%) | Time: {}",
				processed_gb, total_gb, percentage, elapsed_str
			);
		} else {
			eprint!(
				"\rProgress: {:.2}GB processed - {}",
				processed_gb, elapsed_str
			);
		}
	}

	/// Clear the progress display line before printing a log message
	pub fn clear_progress_line(&self) {
		if !self.inner.is_terminal {
			return;
		}

		// ANSI escape code to clear current line, then carriage return
		eprint!("\x1b[2K\r");
	}

	/// Clear the progress display (move to next line on stderr)
	pub fn clear_progress_display(&self) {
		if !self.inner.is_terminal {
			return;
		}

		eprintln!();
	}

	/// Format disk space information as strings
	fn format_disk_space_info(&self) -> Vec<String> {
		if let Ok(info) = self.inner.disk_space.lock() {
			let mut lines = vec![];

			if let Some(initial) = info.initial {
				lines.push(String::new());
				lines.push("Disk Space:".to_string());
				lines.push(format!(
					"  Initial:    {} used of {} total ({} available)",
					ByteSize(initial.used),
					ByteSize(initial.total),
					ByteSize(initial.available)
				));

				if let Some(final_space) = info.final_space {
					let space_used = final_space.used_difference(&initial);
					lines.push(format!(
						"  Final:      {} used of {} total ({} available)",
						ByteSize(final_space.used),
						ByteSize(final_space.total),
						ByteSize(final_space.available)
					));
					if space_used >= 0 {
						lines.push(format!(
							"  Backup used: {} additional space",
							ByteSize(space_used as u64)
						));
					} else {
						lines.push(format!(
							"  Backup freed: {} space",
							ByteSize((-space_used) as u64)
						));
					}
					if let Some(md5_size) = info.md5_store_size {
						lines.push(format!("  MD5 store:   {}", ByteSize(md5_size)));
					}
				}
			}

			lines
		} else {
			vec![]
		}
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
		let stats = BackupStats::new(
			temp_dir.path(),
			"test-session-id",
			0,
			Duration::from_secs(0),
			None,
		);

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
		let stats = BackupStats::new(
			temp_dir.path(),
			"test-session-id",
			0,
			Duration::from_secs(0),
			None,
		);

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
		let stats = BackupStats::new(
			temp_dir.path(),
			"test-session-id",
			0,
			Duration::from_secs(0),
			None,
		);

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
		let stats = BackupStats::new(
			&backup_path,
			"dhb-set-20250929-131320",
			0,
			Duration::from_secs(0),
			None,
		);

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
		assert!(contents.contains("Hardlinked:       1 files"));
		assert!(contents.contains("Copied:           1 files"));
		assert!(contents.contains("Total:            2 files"));
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
