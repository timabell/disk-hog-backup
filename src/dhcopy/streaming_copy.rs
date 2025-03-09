use crossbeam::channel::{Receiver, Sender, bounded};
use md5::Context;
use std::{
	fs::{self, File},
	io::{self, Read, Write},
	path::Path,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicUsize, Ordering},
	},
	thread,
	time::Duration,
};

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
}

impl BackupContext {
	pub fn new(backup_root: &Path) -> Self {
		BackupContext {
			md5_store: None,
			new_md5_store: Md5Store::new(backup_root),
		}
	}

	pub fn with_previous_backup(backup_root: &Path, prev_backup: &Path) -> io::Result<Self> {
		let md5_store = Md5Store::load_from_backup(prev_backup)?;
		let new_md5_store = Md5Store::new(backup_root);

		Ok(BackupContext {
			md5_store: Some(md5_store),
			new_md5_store,
		})
	}

	pub fn save_md5_store(&self) -> io::Result<()> {
		self.new_md5_store.save()
	}
}

pub fn copy_file_with_streaming(
	src_path: &Path,
	dst_path: &Path,
	prev_path: Option<&Path>,
	rel_path: &Path,
	context: &mut BackupContext,
) -> io::Result<bool> {
	// Check if we have a previous backup to compare with
	if let Some(prev) = prev_path {
		if prev.exists() && !prev.is_dir() {
			// First check if file sizes match
			let src_metadata = src_path.metadata()?;
			let prev_metadata = prev.metadata()?;

			if src_metadata.len() == prev_metadata.len() {
				// Check if we have the MD5 hash in the store
				if let Some(md5_store) = &context.md5_store {
					if let Some(prev_hash) = md5_store.get_hash(rel_path) {
						// We have a pre-calculated hash, use it for comparison
						println!("Using pre-calculated hash for {}", rel_path.display());
						let (hardlinked, src_hash) = stream_with_unified_pipeline(
							src_path,
							dst_path,
							prev,
							Some(*prev_hash),
						)?;

						if hardlinked {
							println!("Hardlinked {} (unchanged)", rel_path.display());
						} else {
							println!("Copied {} (changed)", rel_path.display());
						}
						context.new_md5_store.add_hash(rel_path, src_hash);
						return Ok(hardlinked);
					}
				}
				// If we don't have the hash in the store, fall through to regular copy
			}
		}
	}

	// If we get here, either:
	// 1. There's no previous backup
	// 2. The file doesn't exist in the previous backup
	// 3. File sizes don't match
	// 4. We don't have the MD5 hash in the store
	// In these cases, we need to perform a regular streaming copy
	let (_, src_hash) = stream_with_unified_pipeline(src_path, dst_path, Path::new(""), None)?;
	println!("Copied {} (new or no previous hash)", rel_path.display());

	context.new_md5_store.add_hash(rel_path, src_hash);
	Ok(false)
}

// Unified streaming pipeline that reads the file once, computes MD5, and potentially copies
// Returns (hardlinked, src_hash)
fn stream_with_unified_pipeline(
	src_path: &Path,
	dst_path: &Path,
	prev_path: &Path,
	expected_md5: Option<[u8; 16]>,
) -> io::Result<(bool, [u8; 16])> {
	// Create bounded channels for data and MD5 result
	let (data_tx, data_rx) = bounded(MAX_QUEUE_CHUNKS);
	let (md5_tx, md5_rx) = bounded(1);

	// Shared cancellation flag
	let cancel_flag = Arc::new(AtomicBool::new(false));
	let cancel_flag_reader = Arc::clone(&cancel_flag);
	let cancel_flag_writer = Arc::clone(&cancel_flag);

	// Global memory counter
	let global_memory = Arc::new(&GLOBAL_MEMORY_USAGE);
	let global_memory_reader = Arc::clone(&global_memory);
	let global_memory_writer = Arc::clone(&global_memory);

	// Clone paths for threads
	let src_path_clone = src_path.to_path_buf();
	let dst_path_clone = dst_path.to_path_buf();
	let dst_path_for_writer = dst_path_clone.clone();

	// Start reader thread
	let reader_handle = thread::spawn(move || {
		reader_thread(
			&src_path_clone,
			data_tx,
			md5_tx,
			cancel_flag_reader,
			global_memory_reader,
		)
	});

	// Start writer thread
	let writer_handle = thread::spawn(move || {
		writer_thread(
			&dst_path_for_writer,
			data_rx,
			cancel_flag_writer,
			global_memory_writer,
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
	let reader_result = reader_handle.join().unwrap_or_else(|_| {
		Err(io::Error::new(
			io::ErrorKind::Other,
			"Reader thread panicked",
		))
	})?;

	// Wait for writer thread to complete
	let _writer_result = writer_handle.join().unwrap_or_else(|_| {
		Err(io::Error::new(
			io::ErrorKind::Other,
			"Writer thread panicked",
		))
	})?;

	// Check if files matched (if we were monitoring)
	let files_match = if let Some(handle) = monitor_handle {
		handle.join().unwrap_or_else(|_| {
			Err(io::Error::new(
				io::ErrorKind::Other,
				"Monitor thread panicked",
			))
		})?
	} else {
		false
	};

	// If files match, create a hardlink
	if files_match {
		// Create a hardlink from previous to destination
		#[cfg(unix)]
		{
			// Remove the destination file that was created
			fs::remove_file(dst_path)?;
			// Create a hardlink instead
			fs::hard_link(prev_path, dst_path)?;
			return Ok((true, reader_result));
		}

		#[cfg(not(unix))]
		{
			// On non-Unix platforms, fall back to copying
			// Remove the destination file that was created
			fs::remove_file(dst_path)?;
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
	global_memory: Arc<&AtomicUsize>,
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
				let digest = context.compute();
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
	let digest = context.compute();
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
	global_memory: Arc<&AtomicUsize>,
) -> io::Result<()> {
	let mut file = File::create(dst_path)?;

	for chunk in data_rx {
		// Check cancellation flag
		if cancel_flag.load(Ordering::SeqCst) {
			// Clean up and exit
			drop(file);
			fs::remove_file(dst_path)?;
			return Ok(());
		}

		// Write chunk to file
		file.write_all(&chunk)?;

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
	if let Ok(computed_md5) = md5_rx.recv() {
		if computed_md5 == expected_md5 {
			// Files match, signal cancellation
			cancel_flag.store(true, Ordering::SeqCst);
			return Ok(true);
		}
	}
	Ok(false)
}
