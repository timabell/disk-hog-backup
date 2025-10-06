# ADR-001: Unified Streaming Pipeline for File Copy with MD5 Pre-emptive Cancellation

**Date:** 2025-02-21

## Status

Proposed

## Context

In our system we need to copy files from a source disk to a target disk while ensuring that we do not perform unnecessary writes when the file content has not changed (as determined by its MD5 hash). Reading the file twice—once for copying and once for hashing—is inefficient, especially for large files. In addition, if the file is large (e.g., a VM image of 20GB), waiting until the end to compute the MD5 can result in an expensive, ultimately unnecessary write if the source and target already match.

Our goals are to:
- **Avoid duplicate reads for hashing.**
- **Leverage read-ahead** so that the MD5 can be computed early enough to cancel an expensive write if the hash already matches the target.
- **Support both small and large files uniformly** without special casing based on file size.
- **Process multiple files concurrently** while keeping overall memory usage within safe bounds.

This ADR was created by discussing extensively with chatgpt possible approaches.

- https://github.com/timabell/disk-hog-backup/issues/17
- https://chatgpt.com/share/67b8eefc-3564-8006-8f07-fc2fbd31817b

## Decision

We will implement a **unified streaming pipeline** that integrates reading, hashing, and writing in a single pass. The pipeline is designed as follows:

- **Reader Thread:**  
  Reads the file in fixed-size chunks (e.g., 256KB per chunk) and updates the MD5 hasher on the fly. Each chunk is sent into a bounded channel, which serves as a read-ahead buffer. This bounded channel limits how far the reader can get ahead of the writer, ensuring that we do not accumulate excessive data in memory. Once the file is fully read, the reader finalizes the MD5 hash and compares it with the expected hash (if provided). If they match, it sets a cancellation flag to signal the writer to stop processing any remaining buffered chunks.

- **Writer Thread:**  
  Consumes chunks from the channel and writes them to the destination file. Before writing each chunk, the writer checks a shared cancellation flag. If cancellation is signaled (i.e., the computed MD5 matches the expected hash and the reader has set the flag), the writer stops and deletes any partially written file. This allows the writer to avoid writing remaining buffered chunks when the source matches a previous backup.

- **Global Memory Usage Control:**  
  When processing multiple files concurrently, a global memory usage counter (using an `AtomicUsize`) tracks the total bytes buffered across all pipelines. This ensures that the combined read-ahead does not exceed a preset limit (e.g., 4GB).

This two-thread approach automatically adapts:
- For **small files**, the entire file may be buffered before the writer completes. When the reader finishes and finds a hash match, the writer can skip any remaining buffered chunks.
- For **large files**, the reader can get several chunks (up to 32 chunks = 8MB) ahead of the writer. When the reader finishes computing the MD5 and finds a match, the writer can abort processing the remaining buffered chunks, potentially saving writes to slow disks.

## Alternatives Considered

1. **Double-Pass Approach:**  
   Read the file twice—once to compute the MD5 hash and again to perform the write if needed.  
   **Downside:** Increases disk I/O significantly, which is particularly inefficient for large files.

2. **Unbounded Read-Ahead Buffer:**  
   Buffer the entire file (or large portions of it) before starting to write.  
   **Downside:** This can lead to excessive memory usage or out-of-memory errors, especially when processing many files concurrently.

3. **File Size–Based Strategy:**  
   Use a fixed threshold (e.g., 100MB) to decide whether to fully buffer a file for cancellation or stream it as you go.  
   **Downside:** This introduces rigid, special-case logic that does not generalize well across files of varying sizes and becomes problematic when looping over many files with mixed sizes.

4. **Per-File Pipeline Without Concurrency Limits:**  
   Launch a dedicated streaming pipeline (reader, writer, and MD5 monitor) for each file concurrently.  
   **Downside:** Although workable for a small number of files, it does not scale when processing many files simultaneously—risking exhaustion of system resources such as threads and memory.

5. **Parallelizing Reads and Writes Within a Single File:**  
   Use multiple threads to read or write different segments of the same file concurrently.  
   **Downside:** If the write path is the bottleneck, additional threads rarely improve throughput and add significant complexity in coordinating cancellation across threads.

6. **Parallel Reads and Writes Across Many Files:**  
   Process multiple files concurrently by running several file pipelines in parallel.  
   **Downside:** While this can improve overall throughput, it risks saturating disk bandwidth when write operations are the bottleneck, and without effective global memory management, the combined read-ahead may lead to excessive memory usage.

## Rationale

The two-thread streaming pipeline:
- **Eliminates duplicate reads:** Each file is read once for both copying and hashing.
- **Enables selective write cancellation:** When the source MD5 matches a previous backup, the writer can stop processing remaining buffered chunks (up to 8MB), potentially avoiding some writes to slow disks.
- **Manages memory usage effectively:** Bounded channels (32 chunks per file) and a global memory counter ensure that even with multiple files, we remain within safe memory limits.
- **Simplifies architecture:** Using only two threads (reader + writer) eliminates unnecessary coordination complexity. The reader performs MD5 comparison inline after completing the hash, eliminating the need for a separate monitor thread.

## Consequences

- **Performance:**
  - Reduced disk I/O and avoidance of unnecessary writes lead to better overall performance.
  - When MD5 hashes match, the writer can skip up to 8MB of remaining buffered chunks, providing modest savings on slow write paths (especially on HDDs or USB-connected SSDs).

- **Memory Management:**
  - Memory usage is bounded by the channel size (32 chunks × 256KB = 8MB per file) and the global limit (4GB), but proper tuning is essential.

- **Complexity:**
  - Managing two coordinated threads (reader, writer) per file is simple and straightforward.
  - Cancellation logic must be carefully implemented to handle partially written files.

- **Scalability:**  
  - For many files, pipelines can be run concurrently. In production, a thread pool or concurrency limiter would be used to avoid overwhelming system resources.

## Implementation Outline

Below is a simplified Rust example of the approach:

```rust
use crossbeam::channel::{bounded, Receiver, Sender};
use md5::{Context, Digest};
use std::{
    fs::{self, File},
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

const CHUNK_SIZE: usize = 256 * 1024; // 256KB per chunk
const MAX_QUEUE_CHUNKS: usize = 32;     // Limit read-ahead to 32 chunks per file
const GLOBAL_MAX_BUFFER: usize = 4 * 1024 * 1024 * 1024; // 4GB across all files

// Reads the source file in chunks, updating the MD5 hasher and sending chunks to the writer.
fn stream_file(
    src_path: &str,
    data_tx: Sender<Vec<u8>>,
    md5_tx: Sender<(bool, Vec<u8>)>,
    cancel_flag: Arc<AtomicBool>,
    global_memory: Arc<AtomicUsize>,
    expected_md5: Option<Vec<u8>>,
) {
    let mut file = File::open(src_path).expect("Failed to open source file");
    let mut hasher = Context::new();
    let mut buffer = vec![0; CHUNK_SIZE];

    loop {
        let bytes_read = match file.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error reading {}: {}", src_path, e);
                break;
            }
        };

        let chunk = buffer[..bytes_read].to_vec();
        hasher.consume(&chunk);

        // Block if adding this chunk would exceed the global memory cap.
        while global_memory.load(Ordering::SeqCst) + bytes_read > GLOBAL_MAX_BUFFER {
            thread::sleep(Duration::from_millis(10));
        }
        global_memory.fetch_add(bytes_read, Ordering::SeqCst);

        data_tx.send(chunk).expect("Failed to send data chunk");
    }

    // Finalize MD5 and compare with expected if provided
    let final_md5 = hasher.compute();
    let matches = expected_md5.as_ref().map_or(false, |expected| &final_md5.0[..] == &expected[..]);

    // Signal writer to cancel if hashes match
    if matches {
        cancel_flag.store(true, Ordering::SeqCst);
    }

    md5_tx.send((matches, final_md5.0.to_vec())).expect("Failed to send MD5 digest");
}

// Consumes chunks from the channel and writes them to the destination file.
// Stops writing if cancellation is signaled.
fn write_file(
    dst_path: &str,
    data_rx: Receiver<Vec<u8>>,
    cancel_flag: Arc<AtomicBool>,
    global_memory: Arc<AtomicUsize>,
) {
    let mut file = File::create(dst_path).expect("Failed to create destination file");

    for chunk in data_rx.iter() {
        if cancel_flag.load(Ordering::SeqCst) {
            break;
        }

        if let Err(e) = file.write_all(&chunk) {
            eprintln!("Write failed for {}: {}", dst_path, e);
            break;
        }
        global_memory.fetch_sub(chunk.len(), Ordering::SeqCst);
    }

    if cancel_flag.load(Ordering::SeqCst) {
        fs::remove_file(dst_path).expect("Failed to delete canceled file");
    }
}

// Processes a single file by spawning the reader and writer threads.
fn process_file(
    src: &str,
    dst: &str,
    expected_md5: Option<Vec<u8>>,
    global_memory: Arc<AtomicUsize>,
) {
    let (data_tx, data_rx) = bounded::<Vec<u8>>(MAX_QUEUE_CHUNKS);
    let (md5_tx, md5_rx) = bounded::<(bool, Vec<u8>)>(1);
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let src_path = src.to_string();
    let dst_path = dst.to_string();
    let mem_for_reader = Arc::clone(&global_memory);
    let cancel_for_reader = Arc::clone(&cancel_flag);
    let reader_handle = thread::spawn(move || {
        stream_file(&src_path, data_tx, md5_tx, cancel_for_reader, mem_for_reader, expected_md5)
    });

    let mem_for_writer = Arc::clone(&global_memory);
    let cancel_for_writer = Arc::clone(&cancel_flag);
    let writer_handle = thread::spawn(move || {
        write_file(&dst_path, data_rx, cancel_for_writer, mem_for_writer)
    });

    reader_handle.join().unwrap();
    writer_handle.join().unwrap();

    // Get the MD5 comparison result
    let (_matches, _computed_md5) = md5_rx.recv().expect("Failed to receive MD5 result");
}

fn main() {
    // Example list of files to process: (source, destination, expected_md5)
    // For demonstration, the expected MD5 is set to Some(16 zero bytes).
    let files = vec![
        ("source1.file", "dest1.file", Some(vec![0u8; 16])),
        ("source2.file", "dest2.file", Some(vec![0u8; 16])),
        ("source3.file", "dest3.file", None), // No expected hash
    ];

    let global_memory = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for (src, dst, expected_md5) in files {
        let global_mem_clone = Arc::clone(&global_memory);
        let src = src.to_string();
        let dst = dst.to_string();
        let handle = thread::spawn(move || {
            process_file(&src, &dst, expected_md5, global_mem_clone);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
```

## Decision Update: Storing Precomputed MD5 Hashes for Efficient Comparisons

### **Issue: Slow MD5 Recalculation on the Target Backup Set**
The current approach requires re-computing the MD5 of each existing file in the backup set to determine whether the new copy is necessary. This is inefficient, particularly on magnetic disks where reading competes with writes, further slowing down the backup process.

### **Solution: Precomputed MD5 Hash File**
To avoid expensive re-reads from the target disk, we will generate and store a **backup metadata file** containing precomputed MD5 hashes for every file in the backup set. This file will be written in the root of the backup set after each backup completes.

- **Format:** A simple text file listing each file's relative path and its MD5 hash.
- **Location:** Stored in the root directory of each backup set.
- **Usage:**
	- During subsequent backups, this file will be read first to quickly determine whether a file is identical to the previous backup.
	- If the hash matches, we can **hardlink** the file instead of copying it.
	- If the hash does not match, we proceed with reading and hashing the source file to determine whether a write is required.

### **Effect on Performance**
- **Speeds up backup operations:**
	- The MD5 comparison step no longer requires a read from the backup disk for unchanged files.
	- This avoids **contention between reads and writes**, which is especially beneficial for magnetic disks.
- **Hardlinking unchanged files becomes trivial:**
	- Since the hash check is immediate, we can create hardlinks efficiently without re-reading data.
- **Verification is separate and remains trivial:**
	- If verification is required, the MD5 file can be re-checked against the actual files in a separate, explicit step.

### **Implementation Considerations**
- The format should be **human-readable** but also easily parsed programmatically.

This change ensures that **subsequent backups can determine file changes without costly disk reads**, greatly improving efficiency while preserving correctness.
