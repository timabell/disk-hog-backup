package main

import (
	"github.com/timabell/disk-hog-backup/dhcopy"
	"log"
)

func main() {
}

func Backup(source string, dest string) {
	log.Printf("backing up %v into %v\n", source, dest)
	dhcopy.CopyFolder(source, dest)
}
