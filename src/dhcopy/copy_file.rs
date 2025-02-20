use std::fs;
use std::io::{self};
use std::path::Path;

fn copy_file(source: &Path, dest: &Path) -> io::Result<u64> {
	fs::copy(source, dest)
}
