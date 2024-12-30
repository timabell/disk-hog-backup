package backup_sets

import (
	"github.com/timabell/disk-hog-backup/backup_sets"
	"github.com/timabell/disk-hog-backup/dhcopy"
	"log"
	"os"
	"path/filepath"
	"time"
)

func Backup(source string, dest string) (setName string, err error) {
	err = os.MkdirAll(dest, os.ModePerm)
	if err != nil {
		log.Fatal(err)
	}
	setName, err = backup_sets.CreateEmptySet(dest, time.Now)
	if err != nil {
		log.Fatalf("Couldn't create set folder: %s", err)
	}
	destFolder := filepath.Join(dest, setName)
	log.Printf("backing up %v into %v\n", source, destFolder)
	err = dhcopy.CopyFolder(source, destFolder)
	return
}
