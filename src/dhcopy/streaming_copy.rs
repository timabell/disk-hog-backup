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
};

const CHUNK_SIZE: usize = 256 * 1024; // 256KB per chunk
const MAX_QUEUE_CHUNKS: usize = 32; // Limit read-ahead to 32 chunks per file
const GLOBAL_MAX_BUFFER: usize = 4 * 1024 * 1024 * 1024; // 4GB across all files

// Global memory usage counter
static GLOBAL_MEMORY_USAGE: AtomicUsize = AtomicUsize::new(0);

pub fn copy_file_with_streaming(
	src_path: &Path,
	dst_path: &Path,
	prev_path: Option<&Path>,
) -> io::Result<bool> {
	// Check if we should try to hardlink instead of copying
	if let Some(prev) = prev_path {
		if prev.exists() && !prev.is_dir() {
			// First check if file sizes match
			let src_metadata = src_path.metadata()?;
			let prev_metadata = prev.metadata()?;

			if src_metadata.len() == prev_metadata.len() {
				// Files might be the same, proceed with streaming copy
				// that will calculate MD5 and potentially cancel the write
				let hardlinked = stream_with_cancellation(src_path, dst_path, prev)?;
				if hardlinked {
					println!(
						"Hardlinked {} (unchanged)",
						src_path.file_name().unwrap().to_string_lossy()
					);
					return Ok(true);
				}
			}
		}
	}

	// If we get here, we need to perform a regular streaming copy
	println!(
		"Copying {} (new or changed)",
		src_path.file_name().unwrap().to_string_lossy()
	);
	stream_file(src_path, dst_path)?;
	Ok(false)
}

// Stream a file with potential cancellation if MD5 matches previous file
fn stream_with_cancellation(
	src_path: &Path,
	dst_path: &Path,
	prev_path: &Path,
) -> io::Result<bool> {
	// Create bounded channels for data and MD5 result
	let (data_tx, data_rx) = bounded(MAX_QUEUE_CHUNKS);
	let (md5_tx, md5_rx) = bounded(1);

	// Shared cancellation flag
	let cancel_flag = Arc::new(AtomicBool::new(false));
	let cancel_flag_reader = cancel_flag.clone();
	let cancel_flag_writer = cancel_flag.clone();

	// Calculate MD5 of previous file
	let prev_md5 = calculate_md5(prev_path)?;

	// Start reader thread
	let reader_handle = thread::spawn(move || {
		let result = reader_thread(src_path, data_tx, md5_tx, cancel_flag_reader);
		if let Err(e) = &result {
			eprintln!("Reader error for {}: {}", src_path.display(), e);
		}
		result
	});

	// Start writer thread
	let writer_handle = thread::spawn(move || {
		let result = writer_thread(dst_path, data_rx, cancel_flag_writer);
		if let Err(e) = &result {
			eprintln!("Writer error for {}: {}", dst_path.display(), e);
		}
		result
	});

	// Start MD5 monitor thread
	let monitor_handle = thread::spawn(move || {
		if let Ok(computed_md5) = md5_rx.recv() {
			if computed_md5 == prev_md5 {
				// Files match, signal cancellation
				cancel_flag.store(true, Ordering::SeqCst);

				// Wait for writer to finish and clean up
				return Ok(true); // Indicate files match
			}
		}
		Ok(false) // Indicate files don't match
	});

	// Wait for all threads to complete
	let reader_result = reader_handle.join().unwrap_or_else(|_| {
		Err(io::Error::new(
			io::ErrorKind::Other,
			"Reader thread panicked",
		))
	});
	let writer_result = writer_handle.join().unwrap_or_else(|_| {
		Err(io::Error::new(
			io::ErrorKind::Other,
			"Writer thread panicked",
		))
	});
	let monitor_result = monitor_handle.join().unwrap_or_else(|_| {
		Err(io::Error::new(
			io::ErrorKind::Other,
			"Monitor thread panicked",
		))
	});

	// Check for errors
	reader_result?;
	writer_result?;

	// If monitor indicates files match, create a hardlink
	if monitor_result? {
		// Remove the partially written file if it exists
		if dst_path.exists() {
			fs::remove_file(dst_path)?;
		}

		// Create a hardlink from previous to destination
		#[cfg(unix)]
		{
			fs::hard_link(prev_path, dst_path)?;
			return Ok(true);
		}

		#[cfg(not(unix))]
		{
			// On non-Unix platforms, fall back to copying
			fs::copy(prev_path, dst_path)?;
			return Ok(true);
		}
	}

	Ok(false)
}

// Regular streaming copy without cancellation
fn stream_file(src_path: &Path, dst_path: &Path) -> io::Result<()> {
	let mut src_file = File::open(src_path)?;
	let mut dst_file = File::create(dst_path)?;
	let mut buffer = vec![0; CHUNK_SIZE];

	loop {
		let bytes_read = src_file.read(&mut buffer)?;
		if bytes_read == 0 {
			break;
		}
		dst_file.write_all(&buffer[..bytes_read])?;
	}

	Ok(())
}

// Reader thread: reads source file in chunks, updates MD5, sends chunks to writer
fn reader_thread(
	src_path: &Path,
	data_tx: Sender<Vec<u8>>,
	md5_tx: Sender<[u8; 16]>,
	cancel_flag: Arc<AtomicBool>,
) -> io::Result<()> {
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
		while GLOBAL_MEMORY_USAGE.load(Ordering::SeqCst) + bytes_read > GLOBAL_MAX_BUFFER {
			thread::sleep(std::time::Duration::from_millis(10));

			// Check cancellation flag while waiting
			if cancel_flag.load(Ordering::SeqCst) {
				return Ok(());
			}
		}

		// Update global memory usage
		GLOBAL_MEMORY_USAGE.fetch_add(bytes_read, Ordering::SeqCst);

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

	Ok(())
}

// Writer thread: receives chunks from reader and writes them to destination
fn writer_thread(
	dst_path: &Path,
	data_rx: Receiver<Vec<u8>>,
	cancel_flag: Arc<AtomicBool>,
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
		GLOBAL_MEMORY_USAGE.fetch_sub(chunk.len(), Ordering::SeqCst);
	}

	Ok(())
}

// Calculate MD5 hash of a file
fn calculate_md5(path: &Path) -> io::Result<[u8; 16]> {
	let mut file = File::open(path)?;
	let mut context = Context::new();
	let mut buffer = [0; 8192]; // 8KB buffer for reading

	loop {
		let bytes_read = file.read(&mut buffer)?;
		if bytes_read == 0 {
			break;
		}
		context.consume(&buffer[..bytes_read]);
	}

	let digest = context.compute();
	Ok(digest.0)
}
