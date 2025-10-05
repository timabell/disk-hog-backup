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
	pub prev_md5_store: Option<Md5Store>,
	pub new_md5_store: Md5Store,
	pub stats: BackupStats,
}

impl BackupContext {
	pub fn new(backup_root: &Path, session_id: &str) -> Self {
		BackupContext {
			prev_md5_store: None,
			new_md5_store: Md5Store::new(backup_root),
			stats: BackupStats::new(backup_root, session_id),
		}
	}

	pub fn with_previous_backup(
		backup_root: &Path,
		prev_backup: &Path,
		session_id: &str,
	) -> io::Result<Self> {
		let prev_md5_store = Md5Store::load_from_backup(prev_backup)?;
		let new_md5_store = Md5Store::new(backup_root);
		let stats = BackupStats::new(backup_root, session_id);

		Ok(BackupContext {
			prev_md5_store: Some(prev_md5_store),
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

	pub fn print_stats_summary(&self) {
		self.stats.print_summary()
	}
}

// Reference to the same file in the previous backup set
struct PreviousFile<'a> {
	path: &'a Path,
	md5: [u8; 16],
}

struct StreamPipelineArgs<'a> {
	src_path: &'a Path,
	dst_path: &'a Path,
	previous_file: Option<&'a PreviousFile<'a>>,
	stats: &'a BackupStats,
}

pub fn copy_file_with_streaming(
	src_path: &Path,
	dst_path: &Path,
	prev_path: Option<&Path>,
	rel_path: &Path,
	context: &mut BackupContext,
) -> io::Result<bool> {
	let src_metadata = src_path.metadata()?;
	let file_size = src_metadata.len();

	let previous_file = get_matching_previous_file(prev_path, &src_metadata, rel_path, context)?;

	let (hardlinked, src_hash) = stream_with_unified_pipeline(StreamPipelineArgs {
		src_path,
		dst_path,
		previous_file: previous_file.as_ref(),
		stats: &context.stats,
	})?;

	context.new_md5_store.add_hash(rel_path, src_hash);

	if hardlinked {
		context.stats.add_file_hardlinked(file_size);
	} else {
		copy_file_metadata(src_path, dst_path)?;
		context.stats.add_file_copied(file_size);
	}

	if hardlinked {
		println!(
			"  Hardlinked: {} (MD5: {})",
			dst_path.display(),
			format_md5_hash(src_hash)
		);
	} else if let Some(prev) = &previous_file {
		println!(
			"  Copied: {} (MD5 changed: {} -> {})",
			dst_path.display(),
			format_md5_hash(prev.md5),
			format_md5_hash(src_hash)
		);
	} else {
		println!(
			"  Copied: {} (New, MD5: {})",
			dst_path.display(),
			format_md5_hash(src_hash)
		);
	}

	Ok(hardlinked)
}

// get a path & hash for the same file in the previous backup if available and file hasn't changed in size
fn get_matching_previous_file<'a>(
	prev_path: Option<&'a Path>,
	src_metadata: &std::fs::Metadata,
	rel_path: &Path,
	context: &BackupContext,
) -> io::Result<Option<PreviousFile<'a>>> {
	let Some(prev) = prev_path else {
		return Ok(None);
	};
	if !prev.exists() || prev.is_dir() {
		return Ok(None);
	}

	let prev_metadata = prev.metadata()?;
	if src_metadata.len() != prev_metadata.len() {
		return Ok(None);
	}

	let Some(prev_md5_store) = &context.prev_md5_store else {
		return Ok(None);
	};
	let Some(prev_hash) = prev_md5_store.get_hash(rel_path) else {
		return Ok(None);
	};

	Ok(Some(PreviousFile {
		path: prev,
		md5: *prev_hash,
	}))
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
fn stream_with_unified_pipeline(args: StreamPipelineArgs) -> io::Result<(bool, [u8; 16])> {
	let StreamPipelineArgs {
		src_path,
		dst_path,
		previous_file,
		stats,
	} = args;
	let prev_path = previous_file.map(|p| p.path);
	let expected_md5 = previous_file.map(|p| p.md5);

	// Create bounded channels
	// Use Arc to share chunks between writer and hasher without cloning data
	let (writer_tx, writer_rx) = bounded::<Arc<Vec<u8>>>(MAX_QUEUE_CHUNKS);
	let (hasher_tx, hasher_rx) = bounded::<Arc<Vec<u8>>>(MAX_QUEUE_CHUNKS);
	let (md5_tx, md5_rx) = bounded(1);

	let global_memory = &GLOBAL_MEMORY_USAGE;
	let cancel_write = Arc::new(AtomicBool::new(false));

	// Start reader thread
	let src_path_clone = src_path.to_path_buf();
	let reader_stats = stats.clone();
	let reader_handle = thread::spawn(move || {
		reader_thread(
			&src_path_clone,
			writer_tx,
			hasher_tx,
			global_memory,
			&reader_stats,
		)
	});

	// Start hasher thread
	let hasher_stats = stats.clone();
	let hasher_handle = thread::spawn(move || hasher_thread(hasher_rx, md5_tx, &hasher_stats));

	// Start writer thread
	let dst_path_clone = dst_path.to_path_buf();
	let writer_stats = stats.clone();
	let cancel_write_writer = Arc::clone(&cancel_write);
	let writer_handle = thread::spawn(move || {
		writer_thread(
			&dst_path_clone,
			writer_rx,
			cancel_write_writer,
			global_memory,
			&writer_stats,
		)
	});

	// Wait for reader to complete
	reader_handle
		.join()
		.unwrap_or_else(|_| Err(io::Error::other("Reader thread panicked")))?;

	// Wait for hasher to complete and get MD5
	let src_hash = hasher_handle
		.join()
		.unwrap_or_else(|_| Err(io::Error::other("Hasher thread panicked")))?;

	// Receive computed MD5 from channel (hasher already sent it)
	let computed_md5 = md5_rx
		.recv()
		.map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "MD5 channel closed"))?;

	// Compare with expected MD5 and decide on cancellation
	let files_match = expected_md5 == Some(computed_md5);

	if files_match {
		// Signal writer to cancel remaining buffered writes
		cancel_write.store(true, Ordering::SeqCst);
	}

	// Wait for writer to finish
	writer_handle
		.join()
		.unwrap_or_else(|_| Err(io::Error::other("Writer thread panicked")))?;

	// If files match, create a hardlink instead of keeping the written copy
	if files_match && let Some(prev) = prev_path {
		#[cfg(unix)]
		{
			// Remove the destination file that was created
			if dst_path.exists() {
				fs::remove_file(dst_path)?;
			}
			// Create a hardlink instead
			fs::hard_link(prev, dst_path)?;
			return Ok((true, src_hash));
		}

		#[cfg(not(unix))]
		{
			// On non-Unix platforms, fall back to copying
			if dst_path.exists() {
				fs::remove_file(dst_path)?;
			}
			fs::copy(prev, dst_path)?;
			return Ok((true, src_hash));
		}
	}

	Ok((false, src_hash))
}

// Reader thread: reads source file in chunks and sends to both writer and hasher
fn reader_thread(
	src_path: &Path,
	writer_tx: Sender<Arc<Vec<u8>>>,
	hasher_tx: Sender<Arc<Vec<u8>>>,
	global_memory: &AtomicUsize,
	stats: &BackupStats,
) -> io::Result<()> {
	let mut file = File::open(src_path)?;
	let mut buffer = vec![0; CHUNK_SIZE];

	loop {
		// 1. Time read I/O
		let start = std::time::Instant::now();
		let bytes_read = file.read(&mut buffer)?;
		stats.add_reader_io_time(start.elapsed().as_nanos() as u64);

		if bytes_read == 0 {
			break; // EOF
		}

		// Track source bytes read
		stats.add_source_read(bytes_read as u64);

		// Create a chunk wrapped in Arc to share between writer and hasher without cloning data
		let chunk = Arc::new(buffer[..bytes_read].to_vec());

		// 2. Time memory throttle (if any)
		let throttle_start = std::time::Instant::now();
		let mut throttled = false;
		while global_memory.load(Ordering::SeqCst) + bytes_read > GLOBAL_MAX_BUFFER {
			throttled = true;
			thread::sleep(Duration::from_millis(10));
		}
		if throttled {
			let throttle_time = throttle_start.elapsed().as_nanos() as u64;
			stats.add_memory_throttle_time(throttle_time);
			stats.inc_memory_throttle_count();
		}

		// Update global memory usage (counts once, will be decremented by writer)
		global_memory.fetch_add(bytes_read, Ordering::SeqCst);

		// 3. Sample queue depths before send
		stats.sample_writer_queue_depth(writer_tx.len() as u64);
		stats.sample_hasher_queue_depth(hasher_tx.len() as u64);

		// 4. Time send to writer channel
		let start = std::time::Instant::now();
		if writer_tx.send(Arc::clone(&chunk)).is_err() {
			return Err(io::Error::new(
				io::ErrorKind::BrokenPipe,
				"Writer channel closed",
			));
		}
		stats.add_reader_send_writer_time(start.elapsed().as_nanos() as u64);

		// 5. Time send to hasher channel
		let start = std::time::Instant::now();
		if hasher_tx.send(chunk).is_err() {
			return Err(io::Error::new(
				io::ErrorKind::BrokenPipe,
				"Hasher channel closed",
			));
		}
		stats.add_reader_send_hasher_time(start.elapsed().as_nanos() as u64);
	}

	Ok(())
}

// Hasher thread: receives chunks and computes MD5
fn hasher_thread(
	data_rx: Receiver<Arc<Vec<u8>>>,
	md5_tx: Sender<[u8; 16]>,
	stats: &BackupStats,
) -> io::Result<[u8; 16]> {
	let mut context = Context::new();

	// Track receive time by iterating manually
	let mut recv_start = std::time::Instant::now();

	for chunk in data_rx {
		// 1. Time blocked on receive
		stats.add_hasher_recv_time(recv_start.elapsed().as_nanos() as u64);

		stats.add_hashed(chunk.len() as u64);

		// 2. Time MD5 hashing
		let start = std::time::Instant::now();
		context.consume(&*chunk);
		stats.add_hasher_hash_time(start.elapsed().as_nanos() as u64);

		// Reset for next iteration
		recv_start = std::time::Instant::now();
	}

	let digest = context.finalize();

	md5_tx
		.send(digest.0)
		.map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "MD5 channel closed"))?;

	Ok(digest.0)
}

// Writer thread: receives chunks and writes them to destination
fn writer_thread(
	dst_path: &Path,
	data_rx: Receiver<Arc<Vec<u8>>>,
	cancel_write: Arc<AtomicBool>,
	global_memory: &AtomicUsize,
	stats: &BackupStats,
) -> io::Result<()> {
	let mut file = File::create(dst_path)?;

	// Track receive time by iterating manually
	let mut recv_start = std::time::Instant::now();

	for chunk in data_rx {
		// 1. Time blocked on receive
		stats.add_writer_recv_time(recv_start.elapsed().as_nanos() as u64);

		// Check cancellation flag
		if cancel_write.load(Ordering::SeqCst) {
			// Clean up and exit
			drop(file);
			if dst_path.exists() {
				fs::remove_file(dst_path)?;
			}
			return Ok(());
		}

		// 2. Time write I/O
		let start = std::time::Instant::now();
		file.write_all(&chunk)?;
		stats.add_writer_io_time(start.elapsed().as_nanos() as u64);

		// Track target bytes written
		stats.add_target_written(chunk.len() as u64);

		// Update global memory usage
		global_memory.fetch_sub(chunk.len(), Ordering::SeqCst);

		// Reset for next iteration
		recv_start = std::time::Instant::now();
	}

	Ok(())
}
