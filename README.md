# Disk Hog Backup

- Creates backups as normal folders
	- Because you don't want to need fancy tooling to recover your backups in a panic
- Simple, clear command-line interface (CLI)
- Requires minimal user intervention.
- Subsequent backups share identical files as hardlinks with previous backups
	- Saving space without sacrificing backups as normal files and folders
- Automatic checksums - (but you do need to actually check them)
	- Bit-rot is real!
- No encryption / obfuscation / proprietary binary formats
  - Because LVM+LUKS can do that at the filesystem layer and the last thing you want when restoring is a bunch of files you can't read.
- Automatic spotting of lost/deleted/corrupt files since last backup (todo)
- Self-management of disk space (todo)
	- Keeping as many files and versions as possible in the available space, intended to make best use of external USB drives.

# Work in progress!

⚠️ Experimental Alpha. Almost guaranteed to eat all your data currently. Use at
own risk. Make backups before running this anywhere (irony alert). Please report issues at <https://github.com/timabell/disk-hog-backup/issues>

⚠️ This is far from finished, if you want something that already works take a look
at [BackInTime](https://backintime.readthedocs.io/) ([BackInTime repo](https://github.com/bit-team/backintime))

# Usage

```
disk-hog-backup --source <SOURCE> --destination <DESTINATION>
```

## Required Arguments

- `--source <SOURCE>`: The directory to back up
- `--destination <DESTINATION>`: The directory where backups will be stored

# Examples

## Backing up your home directory to an external drive

```bash
# Back up your documents to an external drive
disk-hog-backup --source /home/username/Documents --destination /media/username/ExternalDrive/backups

# Back up your entire home directory
disk-hog-backup --source /home/username --destination /media/username/ExternalDrive/backups
```

## Scheduled Backups with cron

To run backups automatically, you can add a cron job:

```bash
# Edit your crontab
crontab -e

# Add a line to run backups daily at 2 AM
0 2 * * * /usr/local/bin/disk-hog-backup --source /home/username --destination /media/username/ExternalDrive/backups
```

# Verifying Backups

Disk Hog Backup creates MD5 checksums of all backed-up files, making it easy to verify the integrity of your backups using standard tools. The checksums are stored in a file called `disk-hog-backup-hashes.md5`, and a checksum of this file is stored in `disk-hog-backup-hashes.md5.md5`.

To verify your backups:

1. First, verify the integrity of the checksums file itself:

```bash
cd /path/to/backup/set
md5sum -c disk-hog-backup-hashes.md5.md5
# Should output: disk-hog-backup-hashes.md5: OK
```

2. Then verify all files in the backup:

```bash
cd /path/to/backup/set
md5sum -c disk-hog-backup-hashes.md5
# Will check all files listed in the disk-hog-backup-hashes.md5 file
```

This allows you to detect any file corruption that might have occurred since the backup was created.

# Tips

- Mount your external drive to a consistent location for scheduled backups
- Use `.dhbignore` files (see below) to exclude temporary files and large directories you don't need to back up
- Check the backup logs periodically to ensure everything is working correctly
- Periodically verify your backups using the md5sum commands above to ensure data integrity

# Ignoring Files

Disk Hog Backup supports ignoring files and directories using `.dhbignore` files. These files work similarly to `.gitignore` files, allowing you to specify patterns for files and directories that should be excluded from the backup.

## Creating a .dhbignore File

Create a file named `.dhbignore` in the source folder. The ignore patterns will apply to that directory and all its subdirectories.

## Ignore Patterns

The `.dhbignore` file supports the following pattern syntax:

- `#` for comments
- `*` as a wildcard (e.g., `*.tmp` ignores all files with the .tmp extension)
- `/` at the end of a pattern to match directories (e.g., `build/` ignores the build directory)
- `!` at the beginning of a pattern to negate a previous pattern (e.g., `!important.log` includes important.log even if it matches a previous pattern like `*.log`)

Ignore support is provided by the [ignore crate](https://docs.rs/ignore/latest/ignore/).

## Example .dhbignore File

```
# Temporary files
*.tmp
*.temp
*.swp

# Build directories
build/
dist/
node_modules/

# Log files, except important ones
*.log
!important.log
```

# Inspiration

* http://www.mikerubel.org/computers/rsync_snapshots/
* http://rsnapshot.org/
* `rsync --link-dest` hardlink to files in DIR when unchanged
* [BackInTime](https://backintime.readthedocs.io/)
* My own [verify/rehash scripts](https://gist.github.com/timabell/f70f34f8933b2abaf42789f8afdbd7d5)

# Code Design

* [Outside-in-tests](https://pod.0x5.uk/25)
* ADRs - Architecture Decision Records
	* [0001-streaming-copy-and-md5.md](doc/adr/0001-streaming-copy-and-md5.md)
