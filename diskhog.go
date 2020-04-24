package main

import (
	"flag"
	"github.com/timabell/disk-hog-backup/dhcopy"
	"log"
	"os"
)

var source string
var destination string

func main() {
	flag.StringVar(&source, "source", "", "source folder to back up")
	flag.StringVar(&destination, "destination", "", "destination folder for backups")
	flag.Parse()
	Backup(source, destination)
}

func Backup(source string, dest string) error {
	err := os.MkdirAll(dest, os.ModePerm)
	if err != nil {
		log.Fatal(err)
	}
	log.Printf("backing up %v into %v\n", source, dest)
	if err := dhcopy.CopyFolder(source, dest); err != nil {
		return err
	}
	return nil
}
