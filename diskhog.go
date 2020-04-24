package main

import (
	"flag"
	"github.com/timabell/disk-hog-backup/dhcopy"
	"log"
)

var source string
var destination string

func main() {
	flag.StringVar(&source, "source", "", "source folder to back up")
	flag.StringVar(&destination, "destination", "", "destination folder for backup")
	flag.Parse()
	Backup(source, destination)
}

func Backup(source string, dest string) error {
	log.Printf("backing up %v into %v\n", source, dest)
	if err := dhcopy.CopyFolder(source, dest); err != nil {
		return err
	}
	return nil
}
