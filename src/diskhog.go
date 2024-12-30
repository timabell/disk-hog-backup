package main

import (
	"flag"
	backup_sets2 "github.com/timabell/disk-hog-backup/backup"
	"log"
)

var source string
var destination string

func main() {
	flag.StringVar(&source, "source", "", "source folder to back up")
	flag.StringVar(&destination, "destination", "", "destination folder for backups")
	flag.Parse()
	_, err := backup_sets2.Backup(source, destination)
	if err != nil{
		log.Fatalf("Backup failed: %s", err)
	}
}

