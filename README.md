# Disk Hog Backup

This is far from finished, if you want something that already works take a look
at [BackInTime](https://backintime.readthedocs.io/) ([BackInTime repo](https://github.com/bit-team/backintime))

---

Intelligent backups to external disk.

Design goals:

-  Make optimal use of an external
hdd, keeping as much history as possible within the given space.
- Require minimal user intervention.
- Backups are a normal filesystem of files, not requiring any special tools to
  access.
- Backups are verified with checksums stored alongside the backup to allow spotting any bit-rot.

Doesn't even work yet. Almost guaranteed to eat all your data currently. Use at
own risk. Make backups before running this anywhere (irony alert).

# Inspiration

* http://www.mikerubel.org/computers/rsync_snapshots/
* http://rsnapshot.org/
* `rsync --link-dest` hardlink to files in DIR when unchanged
* [BackInTime](https://backintime.readthedocs.io/)
* My own [verify/rehash scripts](https://gist.github.com/timabell/f70f34f8933b2abaf42789f8afdbd7d5)

# Idea

* backup to hotpluggable encrypted compressed external hdd
* use rsnapshot style readable normal folders with hardlinks to use less space
* spot problems by making changes more visible
* keep as much as possible within the limits of the available disk space

# Plan

* first backup
    * copy everything from source to dest - watch out for changing files
* second backup
    * hard link to old backup if same
    * spot dupes, hardlink them
* when down to last xMb (default 100)
    * hardlink files till we hit an unseen thing to back up
      * find the least desirable backup, remove the whole thing in one go
      * continue if enough space else loop

# Code Design

* [Outside-in-tests](https://pod.0x5.uk/25)
* Library-first - to allow this program to be driven from multiple user interfaces, the core logic shall be published as a library crate, and then the bundled CLI will use only the public interface provided by the disk-hog library crate.

# Todo

* automount/unmount external disk for safe removal when not in use - autofs
* disk encryption - luks
* disk compression ??
* scheduling - anacron
* use all available space, overwrite old on demand
* UI for progress
* non-root
* rsnapshot style backup folders
* drilldown report on chnages so you can spot anything untoward
* hardlink moved files
* make old backups read-only to defeat viruses
* optimal plan for removing backups? https://en.wikipedia.org/wiki/Backup_rotation_scheme - "Weighted random distribution"
* how to track what we already have? sha-1 of everything?
* verifying latest backup
* report on primary backup size vs total disk size
* report on available history (simply list dated folders)

# Coding Resources for the future

* Polling filesystem library https://github.com/npat-efault/poller

# Badgers

[![Go](https://github.com/timabell/disk-hog-backup/workflows/Go/badge.svg)](https://github.com/timabell/disk-hog-backup/actions?query=workflow%3AGo)
