#!/bin/sh -v
# requires root
# Make a 100mb ramdisk for manually testing disk space behaviour
mkdir /mnt/ramdisk
mount -t tmpfs -o size=100m tmpfs /mnt/ramdisk
