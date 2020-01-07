package main

import (
	"fmt"
	"github.com/timabell/disk-hog-backup/dhcopy"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
)

func main() {
}

func Backup(source string, dest string) {
	fmt.Printf("backing up %v into %v\n", source, dest)
	contents, err := ioutil.ReadDir(source)
	if err != nil {
		log.Fatal(err)
	}

	for _, item := range contents {
		itemPath := filepath.Join(source, item.Name())
		if item.IsDir() {
			copyFolder(item, dest)
			continue
		}
		destFile := filepath.Join(dest, item.Name())
		dhcopy.CopyFile(itemPath, destFile)
	}
}

func copyFolder(folder os.FileInfo, dest string) {
	destFolder := filepath.Join(dest, folder.Name())
	os.Mkdir(destFolder, 0666)
}
