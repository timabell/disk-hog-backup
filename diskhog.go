package main

import (
	"github.com/timabell/disk-hog-backup/dhcopy"
	"log"
)

func main() {
}

func Backup(source string, dest string) error {
	log.Printf("backing up %v into %v\n", source, dest)
	if err := dhcopy.CopyFolder(source, dest); err != nil {
		return err
	}
	return nil
}
