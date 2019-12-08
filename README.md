# Disk Hog Backup

Intelligent backups to external disk.

The intention is to have a program that will make optimal use of an external
hdd, keeping as much history as possible within the given space; requiring
minimal user intervention.

Doesn't even work yet. Almost guaranteed to eat all your data currently. Use at
own risk. Make backups before running this anywhere.

# Inspiration

* http://www.mikerubel.org/computers/rsync_snapshots/
* http://rsnapshot.org/
* rsync --link-dest hardlink to files in DIR when unchanged

# Idea

* backup to hotpluggable encrypted compressed external hdd
* use rsnapshot style readable normal folders with hardlinks to use less space
* use less space by spotting renamed/moved files
* spot problems by making changes more visible
* keep as much as possible within the limits of the available disk space

# Plan

golang

* first backup
    * copy everything from source to dest - watch out for changing files
    * spot dupes, hardlink them
* second backup
    * hard link to old backup if same
    * spot dupes, hardlink them
* when down to last xMb (default 100)
    * hardlink files till we hit an unseen thing to back up
      * find the least desirable backup, remove the whole thing in one go
      * continue if enough space else loop

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
