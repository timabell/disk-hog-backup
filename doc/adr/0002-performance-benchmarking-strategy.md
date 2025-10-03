# ADR-0002: Pipeline Performance Instrumentation Strategy

## Status
Proposed

## Context

The backup system uses `stream_with_unified_pipeline` which orchestrates a 3-thread pipeline:
- **Reader thread**: Reads 256KB chunks + computes MD5 incrementally + sends via bounded channel (capacity 32)
- **Writer thread**: Receives chunks from channel + writes to destination
- **Monitor thread**: Compares final MD5 with previous backup for hardlinking decisions

### Pipeline Flow Over Time

#### Example 1: No previous file (first backup)

```mermaid
sequenceDiagram
    participant Main

    box rgb(250, 252, 255) Threads
        participant Reader as Reader Thread
        participant Writer as Writer Thread
    end

    participant Channel as Data Channel
    participant SourceDisk as Source Disk
    participant DestDisk as Dest Disk

    Note over Main: First backup - no md5-monitor thread
    Main->>Reader: spawn reader thread
    Main->>Writer: spawn writer thread

    activate Reader
    activate Writer

    loop Each 256KB chunk
        par Reader reads and hashes
            Reader->>SourceDisk: read(chunk)
            SourceDisk-->>Reader: data
            Reader->>Reader: MD5.consume(chunk)
            Reader->>Channel: send(chunk)
        and Writer receives and writes
            Writer->>Channel: recv()
            Channel-->>Writer: chunk
            Writer->>DestDisk: write(chunk)
        end
    end

    Reader->>SourceDisk: read(EOF)
    Reader->>Reader: MD5.finalize()
    Reader->>Main: join()
    deactivate Reader

    Writer->>Channel: recv() returns None
    Writer->>Main: join()
    deactivate Writer
```

#### Example 2: Previous file doesn't match

```mermaid
sequenceDiagram
    participant Main

    box rgb(250, 252, 255) Threads
        participant Reader as Reader Thread
        participant Writer as Writer Thread
        participant Monitor as Monitor MD5 Thread
    end

    participant Channel as Data Channel
    participant SourceDisk as Source Disk
    participant DestDisk as Dest Disk

    Note over Main: File has changed since last backup
    Main->>Reader: spawn reader thread
    Main->>Writer: spawn writer thread
    Main->>Monitor: spawn monitor thread

    activate Reader
    activate Writer
    activate Monitor

    loop Each 256KB chunk
        par Reader and Writer process all chunks
            Reader->>SourceDisk: read(chunk)
            SourceDisk-->>Reader: data
            Reader->>Reader: MD5.consume(chunk)
            Reader->>Channel: send(chunk)
        and
            Writer->>Channel: recv()
            Channel-->>Writer: chunk
            Writer->>DestDisk: write(chunk)
        end
    end

    Reader->>SourceDisk: read(EOF)
    Reader->>Reader: MD5.finalize()
    Reader->>Monitor: send(computed_md5)
    Reader->>Main: join()
    deactivate Reader

    par Monitor compares
        Monitor->>Monitor: compare(computed_md5, expected_md5)
        Note over Monitor: MD5 differs - no match
        Monitor->>Main: join()
        deactivate Monitor
    and
        Writer->>Channel: recv() returns None
        Writer->>Main: join()
        deactivate Writer
    end
```

#### Example 3: Previous file matches - hardlink created

```mermaid
sequenceDiagram
    participant Main

    box rgb(250, 252, 255) Threads
        participant Reader as Reader Thread
        participant Writer as Writer Thread
        participant Monitor as Monitor MD5 Thread
    end

    participant Channel as Data Channel
    participant SourceDisk as Source Disk
    participant DestDisk as Dest Disk

    Note over Main: File unchanged since last backup
    Main->>Reader: spawn reader thread
    Main->>Writer: spawn writer thread
    Main->>Monitor: spawn monitor thread
    Note over Main: All 3 threads now running in parallel

    activate Reader
    activate Writer
    activate Monitor

    loop Each 256KB chunk
        par Reader and Writer run concurrently
            Reader->>SourceDisk: read(chunk)
            SourceDisk-->>Reader: data
            Reader->>Reader: MD5.consume(chunk)
            Reader->>Channel: send(chunk)
            Note over Channel: Queue: 1/32
        and
            Writer->>Channel: recv()
            Channel-->>Writer: chunk
            Writer->>DestDisk: write(chunk)
        end
    end

    Reader->>SourceDisk: read(EOF)
    Reader->>Reader: MD5.finalize()

    Reader->>Monitor: send(computed_md5)
    Reader->>Main: join()
    deactivate Reader

    par Monitor checks MD5 while Writer finishes
        Monitor->>Monitor: compare(computed_md5, expected_md5)
        Monitor->>Writer: cancel_flag.store(true)
        Note over Monitor: MD5 matches!
        Monitor->>Main: join()
        deactivate Monitor
    and
        Writer->>Channel: recv() returns None
        Note over Writer: Checks cancel_flag, stops
        Writer->>Main: join()
        deactivate Writer
    end

    Main->>DestDisk: remove_file(dst_path)
    Main->>DestDisk: hard_link(prev_path, dst_path)
```

#### Example 4: Memory throttle bottleneck

```mermaid
sequenceDiagram
    participant Main

    box rgb(250, 252, 255) Threads
        participant Reader as Reader Thread
        participant Writer as Writer Thread
    end

    participant Channel as Data Channel
    participant SourceDisk as Source Disk
    participant DestDisk as Dest Disk

    Note over Main: Many concurrent file operations hit 4GB limit
    Main->>Reader: spawn reader thread
    Main->>Writer: spawn writer thread

    activate Reader
    activate Writer

    loop Each 256KB chunk
        Reader->>SourceDisk: read(chunk N)
        SourceDisk-->>Reader: data
        Reader->>Reader: MD5.consume(chunk N)

        Note over Reader: GLOBAL_MEMORY_USAGE > 4GB
        Reader->>Reader: spin wait (throttle)
        Note over Reader: BOTTLENECK: Memory backpressure
        Reader->>Reader: wait for memory to free up

        Note over Reader: GLOBAL_MEMORY_USAGE < 4GB
        Reader->>Channel: send(chunk N)

        par Writer processes
            Writer->>Channel: recv()
            Channel-->>Writer: chunk N
            Writer->>DestDisk: write(chunk N)
        end
    end

    Reader->>Main: join()
    deactivate Reader
    Writer->>Main: join()
    deactivate Writer
```

#### Example 5: Write I/O bottleneck (slow destination)

```mermaid
sequenceDiagram
    participant Main

    box rgb(250, 252, 255) Threads
        participant Reader as Reader Thread
        participant Writer as Writer Thread
    end

    participant Channel as Data Channel
    participant SourceDisk as Source Disk
    participant DestDisk as Dest Disk

    Note over Main: Slow USB HDD destination
    Main->>Reader: spawn reader thread
    Main->>Writer: spawn writer thread

    activate Reader
    activate Writer

    loop Each 256KB chunk (hundreds of chunks)
        Reader->>SourceDisk: read(chunk)
        SourceDisk-->>Reader: data (fast SSD)
        Reader->>Reader: MD5.consume(chunk) (fast CPU)

        alt Queue has space
            Reader->>Channel: send(chunk)
            Note over Channel: Queue filling up...
        else Queue full (32/32)
            Note over Reader: Reader blocks on send()
            Note over Reader: BOTTLENECK: Writer too slow
        end

        par Writer slowly processes
            Writer->>Channel: recv()
            Channel-->>Writer: chunk
            Writer->>DestDisk: write(chunk) - SLOW
            Note over Writer: Slow USB HDD write
        end

        alt Queue was full
            Note over Channel: Queue: 31/32 (space freed)
            Channel-->>Reader: send() unblocks
        end
    end

    Reader->>Main: join()
    deactivate Reader
    Writer->>Main: join()
    deactivate Writer
```

#### Example 6: Read I/O bottleneck (slow source)

```mermaid
sequenceDiagram
    participant Main

    box rgb(250, 252, 255) Threads
        participant Reader as Reader Thread
        participant Writer as Writer Thread
    end

    participant Channel as Data Channel
    participant SourceDisk as Source Disk
    participant DestDisk as Dest Disk

    Note over Main: Slow source disk (network mount, old HDD)
    Main->>Reader: spawn reader thread
    Main->>Writer: spawn writer thread

    activate Reader
    activate Writer

    loop Each 256KB chunk (hundreds of chunks)
        par Reader slowly reads
            Reader->>SourceDisk: read(chunk)
            Note over Reader: Slow disk read
            Note over Reader: BOTTLENECK: Read I/O
            SourceDisk-->>Reader: data (finally arrives)
            Reader->>Reader: MD5.consume(chunk)
            Reader->>Channel: send(chunk)
        and Writer waits for data
            Writer->>Channel: recv()
            Note over Writer: Writer blocks on recv()
            Note over Channel: Queue: 0/32 (EMPTY)
            Channel-->>Writer: chunk
            Writer->>DestDisk: write(chunk) (fast)
        end
    end

    Reader->>Main: join()
    deactivate Reader
    Writer->>Main: join()
    deactivate Writer
```

#### Example 7: CPU/Hash bottleneck (slow MD5)

```mermaid
sequenceDiagram
    participant Main

    box rgb(250, 252, 255) Threads
        participant Reader as Reader Thread
        participant Writer as Writer Thread
    end

    participant Channel as Data Channel
    participant SourceDisk as Source Disk
    participant DestDisk as Dest Disk

    Note over Main: Slow CPU, fast disks
    Main->>Reader: spawn reader thread
    Main->>Writer: spawn writer thread

    activate Reader
    activate Writer

    loop Each 256KB chunk (hundreds of chunks)
        par Reader slowly hashes
            Reader->>SourceDisk: read(chunk) (fast)
            SourceDisk-->>Reader: data
            Reader->>Reader: MD5.consume(chunk)
            Note over Reader: Slow CPU hashing
            Note over Reader: BOTTLENECK: MD5 computation
            Reader->>Channel: send(chunk)
        and Writer waits for data
            Writer->>Channel: recv()
            Note over Writer: Writer blocks on recv()
            Note over Channel: Queue: 0/32 (EMPTY)
            Channel-->>Writer: chunk
            Writer->>DestDisk: write(chunk) (fast)
        end
    end

    Reader->>Main: join()
    deactivate Reader
    Writer->>Main: join()
    deactivate Writer
```

### Bottleneck Scenarios

**Scenario A: Write I/O Bound (slow USB HDD)**
- Reader spends most time blocked on `send()` (queue full)
- Writer constantly busy with `write()`
- Queue depth consistently 30-32/32

**Scenario B: CPU Bound (slow CPU, fast SSDs)**
- Reader spends most time in `MD5.consume()`
- Writer often blocked on `recv()` (queue empty)
- Queue depth consistently 0-2/32

**Scenario C: Read I/O Bound (slow source disk)**
- Reader spends most time in `read()`
- Writer often blocked on `recv()` (queue empty)
- Queue depth consistently 0-2/32

### Core Bottleneck Locations
1. **Reader I/O**: `file.read(&mut buffer)` - disk read performance
2. **Reader CPU**: `context.consume(&chunk)` - MD5 computation
3. **Reader blocking**: `data_tx.send(chunk)` - channel full, writer can't keep up
4. **Memory throttle**: Global 4GB limit causes reader to spin-wait
5. **Writer blocking**: Channel receive iterator - reader can't keep up
6. **Writer I/O**: `file.write_all(&chunk)` - disk write performance

Currently we see total bytes but not WHERE time is spent.

## Decision

Instrument the specific bottleneck points in `stream_with_unified_pipeline` to measure time spent in each operation.

### Implementation: Extend BackupStats

Add timing fields to `BackupStatsInner` (src/backup_sets/backup_stats.rs):

```rust
// Pipeline timing (nanoseconds)
reader_io_nanos: AtomicU64,           // Time in file.read()
reader_hash_nanos: AtomicU64,         // Time in MD5 context.consume()
reader_send_nanos: AtomicU64,         // Time blocked on data_tx.send()
writer_recv_nanos: AtomicU64,         // Time blocked on channel receive
writer_io_nanos: AtomicU64,           // Time in file.write_all()
memory_throttle_nanos: AtomicU64,     // Time spinning on memory limit
memory_throttle_count: AtomicU64,     // Number of throttle events
channel_max_depth: AtomicU64,         // Peak queue depth seen
```

### Instrumentation Points

**Reader Thread** (src/dhcopy/streaming_copy.rs, lines ~358-389):

```rust
// Around line 358 - Time pure I/O
let start = Instant::now();
let bytes_read = file.read(&mut buffer)?;
stats.add_reader_io_time(start.elapsed().as_nanos() as u64);

// Around line 371 - Time MD5 computation
let start = Instant::now();
context.consume(&chunk);
stats.add_reader_hash_time(start.elapsed().as_nanos() as u64);

// Around lines 374-383 - Time memory backpressure
let throttle_start = Instant::now();
while GLOBAL_MEMORY_USAGE.load(Ordering::Relaxed) > MAX_MEMORY_USAGE { ... }
stats.add_memory_throttle(throttle_start.elapsed().as_nanos() as u64);

// Around line 389 - Time channel send blocking
let start = Instant::now();
if data_tx.send(chunk).is_err() { break; }
stats.add_reader_send_time(start.elapsed().as_nanos() as u64);

// Every 10 chunks - Sample queue depth
if chunk_count % 10 == 0 {
    stats.update_max_channel_depth(data_tx.len() as u64);
}
```

**Writer Thread** (src/dhcopy/streaming_copy.rs, lines ~420-432):

```rust
// Time channel receive blocking
let recv_start = Instant::now();
for chunk in data_rx {
    stats.add_writer_recv_time(recv_start.elapsed().as_nanos() as u64);

    // Time pure write I/O
    let write_start = Instant::now();
    file.write_all(&chunk)?;
    stats.add_writer_io_time(write_start.elapsed().as_nanos() as u64);

    recv_start = Instant::now(); // Reset for next iteration
}
```

### Display During Backup

Show pipeline utilization every second:
```
[Reader] I/O: 45% | Hash: 35% | Send: 5% | Throttle: 0%
[Writer] Recv: 10% | I/O: 85%
[Queue] Depth: 28/32 | Memory: 1.2GB/4GB
[Rates] Read: 450 MB/s | Write: 89 MB/s
```

**Bottleneck immediately obvious:**
- Reader I/O + Hash = 80%, Writer I/O = 85% → **Both maxed, well balanced**
- Reader = 45%, Writer I/O = 95% → **Write I/O bound**
- Reader Hash = 85%, Writer = 20% → **CPU bound (MD5)**
- Queue always full (30/32) → **Writer can't keep up**
- Throttle events > 0 → **Memory backpressure**

### Single Benchmark

One benchmark calling real `backup()` function:

```rust
fn bench_mixed_workload() {
    // 50 × 1KB files + 5 × 10MB files
    let stats = backup(source, dest)?;

    // Raw metrics show bottleneck
    let total_ms = stats.elapsed.as_millis() as u64;
    println!("Reader I/O: {:.1}%", stats.reader_io_nanos / 1_000_000 * 100 / total_ms);
    println!("Reader hash: {:.1}%", stats.reader_hash_nanos / 1_000_000 * 100 / total_ms);
    println!("Writer I/O: {:.1}%", stats.writer_io_nanos / 1_000_000 * 100 / total_ms);
    println!("Peak queue: {}/32", stats.channel_max_depth);
    println!("Throttle events: {}", stats.memory_throttle_count);
}
```

## Consequences

### Positive
- Pinpoint exact bottleneck location in pipeline
- Uses real backup code, not synthetic tests
- Minimal overhead (just Instant::now() calls)
- Clear before/after comparison for changes

### Negative
- Requires instrumenting production code
- Timing adds small CPU overhead
- More complex than just total time

This gives precise visibility into pipeline performance without changing the core streaming architecture.
