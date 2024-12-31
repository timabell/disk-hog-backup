mod backup;
mod backup_sets;
mod dhcopy;
mod test_helpers;

use clap::Parser;
use std::process;
use crate::backup::backup::backup;

#[derive(Parser)]
#[command(name = "diskhog")]
#[command(about = "A tool for backing up directories", long_about = None)]
struct Args {
    /// Source folder to back up
    #[arg(short, long)]
    source: String,

    /// Destination folder for backups
    #[arg(short, long)]
    destination: String,
}

fn main() {
    let args = Args::parse();

    match backup(&args.source, &args.destination) {
        Ok(_) => println!("Backup successful"),
        Err(e) => {
            eprintln!("Backup failed: {}", e);
            process::exit(1);
        }
    }
}
