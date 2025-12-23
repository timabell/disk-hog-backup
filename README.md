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
- Automatic space management with `--auto-delete`
	- Just-in-time deletion of old backups when disk space runs low
	- Intelligent weighted-random selection favoring deletion of older backups
	- Preserves temporal distribution of backups over time
	- Keeps as many backup versions as possible in available space
	- Perfect for external USB drives and limited storage scenarios

# ⚠️ This is Beta software ⚠️

I've been using this for my own backups for a while and verifying the backups against source hashes and previous backups, squashing bugs as I go, however this hasn't been widely tested by the community so you MUST ensure you have alternative and current backups before using this software.

As per the A-GPL license (see sections 15 & 16) under which this software is licensed - this software is used entirely at your own risk and comes with no warranty/liability for correct behaviour or data loss.

Please report any issues encountered at <https://github.com/timabell/disk-hog-backup/issues>

It is recommended to follow at a minimum the [3-2-1 backup rule](https://en.wikipedia.org/wiki/Backup#3-2-1_Backup_Rule) to ensure the failure of a single source/backup cannot result in complete data loss, regardless of the backup tools you choose.

# Usage

```
disk-hog-backup --source <SOURCE> --destination <DESTINATION>
```

## Required Arguments

- `--source <SOURCE>`: The directory to back up
- `--destination <DESTINATION>`: The directory where backups will be stored

## Optional Arguments

- `--auto-delete`: Enable automatic deletion of old backups when disk space is low

# Examples

## Backing up your home directory to an external drive

```bash
# Back up your documents to an external drive
disk-hog-backup --source /home/username/Documents --destination /media/username/ExternalDrive/backups

# Back up your entire home directory
disk-hog-backup --source /home/username --destination /media/username/ExternalDrive/backups
```

## Using Auto-Delete for Limited Storage

When backing up to external drives or limited storage devices, use the `--auto-delete` flag to automatically manage space:

```bash
# Automatically delete old backups when space runs low
disk-hog-backup --source /home/username/Documents \
                --destination /media/username/ExternalDrive/backups \
                --auto-delete
```

### How Auto-Delete Works

The `--auto-delete` feature intelligently manages disk space by:

1. **Just-in-time detection**: Before copying each file, checks if there's sufficient space available
2. **Smart deletion**: When space is low, automatically deletes old backup sets using a weighted-random algorithm that:
   - Favors deletion of older backups
   - Preserves good temporal distribution across all backups
   - Handles irregular backup schedules gracefully
   - Always keeps at least one previous backup (for hard-linking)
3. **Transparent operation**: Shows clear messages when auto-deletion occurs, including which backup set was deleted

**Example scenario**: Backing up to a 100MB external drive with 10MB files:
- First backup: Creates backup set #1 (10MB used)
- Second backup: Creates backup set #2 (20MB used - files hard-linked where unchanged)
- Continues until disk is nearly full...
- When space runs low: Automatically deletes oldest backup set before copying next file
- Result: Maintains as many backup versions as possible within available space

See [ADR-004](doc/adr/0004-automatic-space-management-and-testing-strategy.md) for implementation details.

## Scheduled Backups with cron

To run backups automatically, you can add a cron job:

```bash
# Edit your crontab
crontab -e

# Add a line to run backups daily at 2 AM
0 2 * * * /usr/local/bin/disk-hog-backup --source /home/username --destination /media/username/ExternalDrive/backups

# Or with auto-delete for limited storage
0 2 * * * /usr/local/bin/disk-hog-backup --source /home/username --destination /media/username/ExternalDrive/backups --auto-delete
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

# Spotting lost/corrupt files (bit-rot)

If you accidentally delete a file or it becomes corrupted on your source disk before, it might be rotated out of your backups and lost forever before you realize.

My solution to this is supported by the hash files generated by disk-hog-backup but is not part of the tool itself. The way I deal with it is that once the backup has completed and generated new hashes for everything that was backed up, I run my [awol-hashes.sh](https://gist.github.com/timabell/f7f776c7f0792ea13ef44798082b9935) shell script, which diffs the most recent checksums with the previous backup's checksums.

This tool uses [my fork of md5-tools](https://github.com/timabell/md5-tools) and a little utility called [paths2html](https://github.com/timabell/paths2html) that I created to generate an collapsible view of every hash (i.e. file contents) that once existed but is no longer anywhere in the latest backup set.

By reviewing the generated list of missing hashes I can quickly spot any unexpected lost data and see areas of change due to intentional modification/deletion.

# Tips

- Mount your external drive to a consistent location for scheduled backups
- Use `.dhbignore` files (see below) to exclude temporary files and large directories you don't need to back up
- Check the backup logs periodically to ensure everything is working correctly
- Periodically verify your backups using the md5sum commands above to ensure data integrity
- For limited storage devices (external USB drives, etc.), use `--auto-delete` to maximize the number of backup versions that fit
- The `--auto-delete` feature is opt-in to prevent surprise deletions - only enable it when you understand the behavior

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

## Example .dhbignore File

```
# ignore anywhere
.cache/
.thumbnails/

# ignore at root of source only
/.asdf
/.dbus
/.dropbox
/.gvs
/.hplip
/.java
/.local/share/Trash/
/.npm
/.nuget
/VirtalBox VMs
/docker
/no-sync

# ignore flatpack installs
/.var/app

# ignore Virtual disk images anywhere
*.vdi


# This is ~/tmp not actually /tmp
/tmp

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
* [Architecture Decision Records (ADRs)](doc/adr/)
* [Threading Architecture](doc/threading.md) - Multi-threaded pipeline design for backup operations

# Alternative tools

* [rclone](https://github.com/rclone/rclone) - focus on cloud sync but can presumably do local too
* [restic](https://github.com/restic/restic) - encrypted backups on local & cloud
