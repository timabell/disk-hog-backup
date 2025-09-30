use crossbeam::channel::{Receiver, Sender, bounded};
use md5::Context;
use std::{
	fmt::Write,
	fs::{self, File},
	io::{self, Read, Write as IoWrite},
	path::Path,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicUsize, Ordering},
	},
	thread,
	time::Duration,
};

use crate::backup_sets::backup_stats::BackupStats;
use crate::backup_sets::md5_store::Md5Store;

const CHUNK_SIZE: usize = 256 * 1024; // 256KB per chunk
const MAX_QUEUE_CHUNKS: usize = 32; // Limit read-ahead to 32 chunks per file
const GLOBAL_MAX_BUFFER: usize = 4 * 1024 * 1024 * 1024; // 4GB across all files

// Global memory usage counter
static GLOBAL_MEMORY_USAGE: AtomicUsize = AtomicUsize::new(0);

// Struct to hold context for a backup operation
pub struct BackupContext {
	pub md5_store: Option<Md5Store>,
	pub new_md5_store: Md5Store,
	pub stats: BackupStats,
}

impl BackupContext {
	pub fn new(backup_root: &Path, session_id: &str) -> Self {
		BackupContext {
			md5_store: None,
			new_md5_store: Md5Store::new(backup_root),
			stats: BackupStats::new(backup_root, session_id),
		}
	}

	pub fn with_previous_backup(
		backup_root: &Path,
		prev_backup: &Path,
		session_id: &str,
	) -> io::Result<Self> {
		let md5_store = Md5Store::load_from_backup(prev_backup)?;
		let new_md5_store = Md5Store::new(backup_root);
		let stats = BackupStats::new(backup_root, session_id);

		Ok(BackupContext {
			md5_store: Some(md5_store),
			new_md5_store,
			stats,
		})
	}

	pub fn save_md5_store(&self) -> io::Result<()> {
		self.new_md5_store.save()
	}

	pub fn save_stats(&self) -> io::Result<()> {
		self.stats.save()
	}
}

pub fn copy_file_with_streaming(
	src_path: &Path,
	dst_path: &Path,
	prev_path: Option<&Path>,
	rel_path: &Path,
	context: &mut BackupContext,
) -> io::Result<bool> {
	// Get the source file size for statistics
	let src_metadata = src_path.metadata()?;
	let file_size = src_metadata.len();

	// Check if we have a previous backup to compare with
	if let Some(prev) = prev_path
		&& prev.exists()
		&& !prev.is_dir()
	{
		// First check if file sizes match
		let prev_metadata = prev.metadata()?;

		if src_metadata.len() == prev_metadata.len() {
			// Check if we have the MD5 hash in the store
			if let Some(md5_store) = &context.md5_store
				&& let Some(prev_hash) = md5_store.get_hash(rel_path)
			{
				// We have a pre-calculated hash, use it for comparison
				let prev_hash_hex = format_md5_hash(*prev_hash);

				// Stream the file with unified pipeline
				let (hardlinked, src_hash) = stream_with_unified_pipeline(
					src_path,
					dst_path,
					prev,
					Some(*prev_hash),
					&context.stats,
				)?;

				let src_hash_hex = format_md5_hash(src_hash);

				if hardlinked {
					// Track hardlinked file (space saved)
					context.stats.add_file_hardlinked(file_size);
					println!(
						"  Hardlinked: {} (MD5: {})",
						dst_path.display(),
						src_hash_hex
					);
				} else {
					// Track copied file (new data written)
					context.stats.add_file_copied(file_size);
					println!(
						"  Copied: {} (MD5 changed: {} -> {})",
						dst_path.display(),
						prev_hash_hex,
						src_hash_hex
					);
				}

				context.new_md5_store.add_hash(rel_path, src_hash);

				// If we didn't hardlink, we need to preserve the metadata
				if !hardlinked {
					copy_file_metadata(src_path, dst_path)?;
				}

				return Ok(hardlinked);
			}
			// If we don't have the hash in the store, fall through to regular copy
		}
	}

	// If we get here, either:
	// 1. There's no previous backup
	// 2. The file doesn't exist in the previous backup
	// 3. File sizes don't match
	// 4. We don't have the MD5 hash in the store
	// In these cases, we need to perform a regular streaming copy
	let (_, src_hash) =
		stream_with_unified_pipeline(src_path, dst_path, Path::new(""), None, &context.stats)?;

	// Track copied file (new data written)
	context.stats.add_file_copied(file_size);

	let src_hash_hex = format_md5_hash(src_hash);
	println!(
		"  Copied: {} (New, MD5: {})",
		dst_path.display(),
		src_hash_hex
	);

	// Preserve file metadata (timestamps, permissions)
	copy_file_metadata(src_path, dst_path)?;

	context.new_md5_store.add_hash(rel_path, src_hash);
	Ok(false)
}

// Helper function to copy file metadata (timestamps, permissions)
fn copy_file_metadata(src_path: &Path, dst_path: &Path) -> io::Result<()> {
	let src_metadata = fs::metadata(src_path)?;

	// Copy file permissions
	#[cfg(unix)]
	{
		let permissions = src_metadata.permissions();
		fs::set_permissions(dst_path, permissions)?;
	}

	// Copy file timestamps
	#[cfg(unix)]
	{
		use std::os::unix::fs::MetadataExt;
		let atime = filetime::FileTime::from_unix_time(
			src_metadata.atime(),
			src_metadata.atime_nsec() as u32,
		);
		let mtime = filetime::FileTime::from_unix_time(
			src_metadata.mtime(),
			src_metadata.mtime_nsec() as u32,
		);
		filetime::set_file_times(dst_path, atime, mtime)?;
	}

	#[cfg(windows)]
	{
		use std::os::windows::fs::MetadataExt;
		let last_write_time = src_metadata.last_write_time();
		let last_access_time = src_metadata.last_access_time();
		let creation_time = src_metadata.creation_time();

		// Convert Windows time to FileTime
		let mtime = filetime::FileTime::from_windows_file_time(last_write_time);
		let atime = filetime::FileTime::from_windows_file_time(last_access_time);

		filetime::set_file_times(dst_path, atime, mtime)?;
	}

	Ok(())
}

// Helper function to format MD5 hash as a hex string
fn format_md5_hash(hash: [u8; 16]) -> String {
	hash.iter().fold(String::new(), |mut output, b| {
		let _ = write!(output, "{:02x}", b);
		output
	})
}

// Unified streaming pipeline that reads the file once, computes MD5, and potentially copies
// Returns (hardlinked, src_hash)
fn stream_with_unified_pipeline(
	src_path: &Path,
	dst_path: &Path,
	prev_path: &Path,
	expected_md5: Option<[u8; 16]>,
	stats: &BackupStats,
) -> io::Result<(bool, [u8; 16])> {
	// Create bounded channels for data and MD5 result
	let (data_tx, data_rx) = bounded(MAX_QUEUE_CHUNKS);
	let (md5_tx, md5_rx) = bounded(1);

	// Shared cancellation flag
	let cancel_flag = Arc::new(AtomicBool::new(false));
	let cancel_flag_reader = Arc::clone(&cancel_flag);
	let cancel_flag_writer = Arc::clone(&cancel_flag);

	// Global memory counter
	let global_memory = &GLOBAL_MEMORY_USAGE;

	// Clone paths for threads
	let src_path_clone = src_path.to_path_buf();
	let dst_path_clone = dst_path.to_path_buf();
	let dst_path_for_writer = dst_path_clone.clone();

	// Clone stats for threads
	let reader_stats = stats.clone();
	let writer_stats = stats.clone();

	// Start reader thread
	let reader_handle = thread::spawn(move || {
		reader_thread(
			&src_path_clone,
			data_tx,
			md5_tx,
			cancel_flag_reader,
			global_memory,
			&reader_stats,
		)
	});

	// Start writer thread
	let writer_handle = thread::spawn(move || {
		writer_thread(
			&dst_path_for_writer,
			data_rx,
			cancel_flag_writer,
			global_memory,
			&writer_stats,
		)
	});

	// Start MD5 monitor thread if we have an expected hash
	let monitor_handle = if let Some(expected) = expected_md5 {
		let cancel_flag_monitor = Arc::clone(&cancel_flag);
		Some(thread::spawn(move || {
			monitor_md5(md5_rx, cancel_flag_monitor, expected)
		}))
	} else {
		// No expected hash, just consume the MD5 result
		None
	};

	// Wait for reader thread to complete
	let reader_result = reader_handle
		.join()
		.unwrap_or_else(|_| Err(io::Error::other("Reader thread panicked")))?;

	// Wait for writer thread to finish
	writer_handle
		.join()
		.unwrap_or_else(|_| Err(io::Error::other("Writer thread panicked")))?;

	// Check if files matched (if we were monitoring)
	let files_match = if let Some(handle) = monitor_handle {
		handle
			.join()
			.unwrap_or_else(|_| Err(io::Error::other("Monitor thread panicked")))?
	} else {
		false
	};

	// If files match, create a hardlink
	if files_match {
		// Create a hardlink from previous to destination
		#[cfg(unix)]
		{
			// Check if destination file exists before trying to remove it
			if dst_path.exists() {
				// Remove the destination file that was created
				fs::remove_file(dst_path)?;
			}
			// Create a hardlink instead
			fs::hard_link(prev_path, dst_path)?;
			return Ok((true, reader_result));
		}

		#[cfg(not(unix))]
		{
			// On non-Unix platforms, fall back to copying
			// Check if destination file exists before trying to remove it
			if dst_path.exists() {
				// Remove the destination file that was created
				fs::remove_file(dst_path)?;
			}
			// Copy the file instead
			fs::copy(prev_path, dst_path)?;
			return Ok((true, reader_result));
		}
	}

	// Get the source hash from the reader thread
	Ok((false, reader_result))
}

// Reader thread: reads source file in chunks, updates MD5, sends chunks to writer
// Returns the calculated MD5 hash
fn reader_thread(
	src_path: &Path,
	data_tx: Sender<Vec<u8>>,
	md5_tx: Sender<[u8; 16]>,
	cancel_flag: Arc<AtomicBool>,
	global_memory: &AtomicUsize,
	stats: &BackupStats,
) -> io::Result<[u8; 16]> {
	let mut file = File::open(src_path)?;
	let mut context = Context::new();
	let mut buffer = vec![0; CHUNK_SIZE];

	loop {
		// Check cancellation flag
		if cancel_flag.load(Ordering::SeqCst) {
			break;
		}

		// Read a chunk
		let bytes_read = file.read(&mut buffer)?;
		if bytes_read == 0 {
			break; // EOF
		}

		// Track source bytes read
		stats.add_source_read(bytes_read as u64);
		stats.add_hashed(bytes_read as u64);

		// Create a chunk to send
		let chunk = buffer[..bytes_read].to_vec();

		// Update MD5 hasher
		context.consume(&chunk);

		// Wait if global memory usage is too high
		while global_memory.load(Ordering::SeqCst) + bytes_read > GLOBAL_MAX_BUFFER {
			thread::sleep(Duration::from_millis(10));

			// Check cancellation flag while waiting
			if cancel_flag.load(Ordering::SeqCst) {
				// Return the current MD5 hash
				let digest = context.finalize();
				return Ok(digest.0);
			}
		}

		// Update global memory usage
		global_memory.fetch_add(bytes_read, Ordering::SeqCst);

		// Send chunk to writer
		if data_tx.send(chunk).is_err() {
			// Channel closed, writer probably failed
			return Err(io::Error::new(
				io::ErrorKind::BrokenPipe,
				"Writer channel closed",
			));
		}
	}

	// Compute final MD5 and send it
	let digest = context.finalize();
	if md5_tx.send(digest.0).is_err() {
		return Err(io::Error::new(
			io::ErrorKind::BrokenPipe,
			"MD5 channel closed",
		));
	}

	Ok(digest.0)
}

// Writer thread: receives chunks from reader and writes them to destination
fn writer_thread(
	dst_path: &Path,
	data_rx: Receiver<Vec<u8>>,
	cancel_flag: Arc<AtomicBool>,
	global_memory: &AtomicUsize,
	stats: &BackupStats,
) -> io::Result<()> {
	let mut file = File::create(dst_path)?;

	for chunk in data_rx {
		// Check cancellation flag
		if cancel_flag.load(Ordering::SeqCst) {
			// Clean up and exit
			drop(file);
			if dst_path.exists() {
				fs::remove_file(dst_path)?;
			}
			return Ok(());
		}

		// Write chunk to file
		file.write_all(&chunk)?;

		// Track target bytes written
		stats.add_target_written(chunk.len() as u64);

		// Update global memory usage
		global_memory.fetch_sub(chunk.len(), Ordering::SeqCst);
	}

	Ok(())
}

// Monitor thread: compares computed MD5 with expected hash and signals cancellation if they match
fn monitor_md5(
	md5_rx: Receiver<[u8; 16]>,
	cancel_flag: Arc<AtomicBool>,
	expected_md5: [u8; 16],
) -> io::Result<bool> {
	if let Ok(computed_md5) = md5_rx.recv()
		&& computed_md5 == expected_md5
	{
		// Files match, signal cancellation
		cancel_flag.store(true, Ordering::SeqCst);
		return Ok(true);
	}
	Ok(false)
}
