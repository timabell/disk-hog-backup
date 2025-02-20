mod backup;
mod backup_sets;
mod dhcopy;

use crate::backup::backup;
use clap::Parser;
use std::process;

#[derive(Parser)]
#[command(about = "A tool for backing up directories", long_about = None)]
#[clap(author, version)]
struct Args {
	/// Source folder to back up
	#[arg(short, long)]
	source: String,

	/// Destination folder for backups
	#[arg(short, long)]
	destination: String,
}

fn main() {
	println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
	println!("License: {}", env!("CARGO_PKG_LICENSE"));
	println!("{}", env!("CARGO_PKG_REPOSITORY"));
	println!();

	let args = Args::parse();

	match backup(&args.source, &args.destination) {
		Ok(_) => (),
		Err(e) => {
			eprintln!("Backup failed: {}", e);
			process::exit(1);
		}
	}
}
