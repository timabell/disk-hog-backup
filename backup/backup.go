package backup

import (
	"github.com/timabell/disk-hog-backup/backup_sets"
	"github.com/timabell/disk-hog-backup/dhcopy"
	"github.com/timabell/disk-hog-backup/hard_linker"
	"log"
	"os"
	"path/filepath"
	"time"
)

func Backup(source string, dest string, getTime func () (time.Time)) (setName string, err error) {
	err = os.MkdirAll(dest, os.ModePerm)
	if err != nil {
		log.Fatal(err)
	}
	lastSetName, err := backup_sets.FindLatestSet(dest)
	if err != nil {
		log.Fatalf("Failed to search for previous backup set: %s", err)
	}
	setName, err = backup_sets.CreateEmptySet(dest, getTime)
	if err != nil {
		log.Fatalf("Couldn't create set folder: %s", err)
	}
	destFolder := filepath.Join(dest, setName)
	if lastSetName != "" {
		lastSetPath := filepath.Join(dest, lastSetName)
		hard_linker.HardLinkCopy(lastSetPath, destFolder)
	}
	log.Printf("backing up %v into %v\n", source, destFolder)
	err = dhcopy.CopyFolder(source, destFolder)
	return
}
