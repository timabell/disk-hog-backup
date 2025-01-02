# Disk Hog Backup

- Creates backups as normal folders
	- Because you don't want to need fancy tooling to recover your backups in a panic
- Subsequent backups share identical files as hardlinks with previous backups
	- Saving space without sacrificing backups as normal files and folders
- Self-management of disk space
	- Keeping as many files and versions as possible in the available space, intended to make best use of external USB drives.
- No encryption
  - Because LVM+LUKS can do that at the filesystem layer
- Automatic checksums and validation of new and existing backups
	- Bit-rot is real
- Require minimal user intervention.
- Simple, clear command-line interface (CLI)
- Available as a library (rust crate) to allow other user interfaces / automations to be built
- Reports of files that have gone missing based on previous checksums
	- spot problems by making changes more visible

## Work in progress

⚠️ Doesn't even work yet. Almost guaranteed to eat all your data currently. Use at
own risk. Make backups before running this anywhere (irony alert).

⚠️ This is far from finished, if you want something that already works take a look
at [BackInTime](https://backintime.readthedocs.io/) ([BackInTime repo](https://github.com/bit-team/backintime))

# Inspiration

* http://www.mikerubel.org/computers/rsync_snapshots/
* http://rsnapshot.org/
* `rsync --link-dest` hardlink to files in DIR when unchanged
* [BackInTime](https://backintime.readthedocs.io/)
* My own [verify/rehash scripts](https://gist.github.com/timabell/f70f34f8933b2abaf42789f8afdbd7d5)

# Code Design

* [Outside-in-tests](https://pod.0x5.uk/25)
* Library-first - to allow this program to be driven from multiple user interfaces, the core logic shall be published as a library crate, and then the bundled CLI will use only the public interface provided by the disk-hog library crate.
