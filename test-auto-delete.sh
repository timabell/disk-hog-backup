#!/bin/bash
set -e

echo "========================================"
echo "Auto-delete test script"
echo "========================================"
echo ""
echo "This script demonstrates the just-in-time auto-delete feature."
echo "It will:"
echo "  1. Create a source directory with 10MB test files"
echo "  2. Run backups repeatedly to fill the 100MB target disk"
echo "  3. Show auto-delete triggering when space runs low"
echo "  4. Continue creating successful backups despite limited space"
echo ""
echo "Expected progression (86MB disk, 10MB files):"
echo ""
echo "Iter | Source Files | Size | Action              | Sets | Total | Notes"
echo "-----|--------------|------|---------------------|------|-------|------------------"
echo "1    | file001      | 10MB | First backup        | 1    | ~10MB | Nothing to link"
echo "2    | file001-002  | 20MB | Add file002         | 2    | ~30MB | file001 linked"
echo "3    | file001-003  | 30MB | Add file003         | 3    | ~60MB | file001-002 linked"
echo "4    | file002-004  | 30MB | Add 004, del 001    | 4    | ~90MB | Getting full"
echo "5    | file003-005  | 30MB | Add 005, del 002    | 3    | ~60MB | AUTO-DELETE set 1"
echo "6    | file004-006  | 30MB | Add 006, del 003    | 3    | ~60MB | AUTO-DELETE set 2"
echo "7+   | ...          | 30MB | Add 1, del 1        | 3    | ~60MB | AUTO-DELETE each"
echo ""
echo "Strategy: Keep source at 30MB (3 files) to allow 2-3 backup sets + overhead"
echo ""

SOURCE_DIR="/tmp/dhb-src-$(date +%s)"
DEST_DIR="/media/tim/small/full-disk-test"

echo "Source: $SOURCE_DIR"
echo "Destination: $DEST_DIR"
echo ""

# Check if destination parent exists
DEST_PARENT=$(dirname "$DEST_DIR")
if [ ! -d "$DEST_PARENT" ]; then
	echo "ERROR: Destination parent directory $DEST_PARENT does not exist"
	echo "Please create a 100MB test disk and mount it at $DEST_PARENT"
	exit 1
fi

echo "Checking destination disk size..."
DISK_SIZE=$(df -BM "$DEST_DIR" | tail -1 | awk '{print $2}' | sed 's/M//')
echo "Destination disk size: ${DISK_SIZE}MB"
echo ""

if [ "$DISK_SIZE" -lt 90 ] || [ "$DISK_SIZE" -gt 110 ]; then
	echo "WARNING: Expected ~100MB disk, got ${DISK_SIZE}MB"
	echo "Continuing anyway..."
	echo ""
fi

# Create source directory
echo "Creating source directory..."
mkdir -p "$SOURCE_DIR"
echo "Done."
echo ""

# Function to create a random 10MB file
create_test_file() {
	local filename="$1"
	echo "  Creating $filename (10MB)..."
	dd if=/dev/urandom of="$SOURCE_DIR/$filename" bs=1M count=10 2>/dev/null
}

# Function to show disk space
show_disk_space() {
	echo ""
	echo "----------------------------------------"
	echo "Current disk usage on $DEST_DIR:"
	df -h "$DEST_DIR" | tail -1 | awk '{printf "  Total: %s  Used: %s  Available: %s  Use%%: %s\n", $2, $3, $4, $5}'
	echo "Backup sets in destination:"
	ls -lh "$DEST_DIR" | grep "^d" | wc -l | awk '{printf "  Count: %d\n", $1}'
	if [ -d "$DEST_DIR" ]; then
		ls -lh "$DEST_DIR" | grep "^d" | awk '{print "  - " $9}' || echo "  (none yet)"
	fi
	echo "----------------------------------------"
	echo ""
}

# Function to run backup
run_backup() {
	local run_num="$1"
	echo ""
	echo "========================================"
	echo "BACKUP RUN #$run_num"
	echo "========================================"
	echo ""

	show_disk_space

	echo "Running: cargo run -- --source $SOURCE_DIR --destination $DEST_DIR --auto-delete"
	echo ""
	cargo run -- --source "$SOURCE_DIR" --destination "$DEST_DIR" --auto-delete

	echo ""
	echo "Backup run #$run_num completed successfully!"
	show_disk_space
}

# Run multiple backup iterations
for i in {1..10}; do
	echo ""
	echo "========================================"
	echo "ITERATION $i - Adding files and running backup"
	echo "========================================"
	echo ""

	# Add 1 new file each iteration
	file_num=$(printf "%03d" $i)

	echo "Adding new file to source:"
	create_test_file "file${file_num}.bin"
	echo ""

	# Delete oldest file to keep source size stable at ~30MB (3 files)
	if [ $i -gt 3 ]; then
		delete_candidate=$(ls "$SOURCE_DIR" | head -1)
		if [ -n "$delete_candidate" ]; then
			echo "Deleting oldest file from source: $delete_candidate"
			rm "$SOURCE_DIR/$delete_candidate"
			echo ""
		fi
	fi

	echo "Current source directory contents:"
	ls -lh "$SOURCE_DIR" | tail -n +2 | awk '{print "  " $9 " (" $5 ")"}'
	echo ""

	# Run backup
	run_backup $i

	# Small delay to make timestamps different
	sleep 1
done

echo ""
echo "========================================"
echo "TEST COMPLETE"
echo "========================================"
echo ""
echo "Summary:"
echo "  - Successfully ran 10 backup iterations"
echo "  - Auto-delete should have triggered multiple times"
echo "  - Check the output above to see when backup sets were deleted"
echo ""
show_disk_space
echo ""
echo "You can manually inspect:"
echo "  Source: $SOURCE_DIR"
echo "  Destination: $DEST_DIR"
echo ""
