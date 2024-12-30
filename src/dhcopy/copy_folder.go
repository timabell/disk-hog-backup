package dhcopy

import (
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
)

func CopyFolder(source string, dest string) error {
	log.Printf("backing up folder %v into %v\n", source, dest)
	contents, err := ioutil.ReadDir(source)
	if err != nil {
		log.Fatal(err)
	}

	for _, item := range contents {
		if item.IsDir() {
			destFolder := filepath.Join(dest, item.Name())
			err := os.Mkdir(destFolder, os.ModePerm)
			if err != nil {
				log.Fatal(err)
			}
			dirPath := filepath.Join(source, item.Name())
			if err := CopyFolder(dirPath, destFolder); err != nil {
				return err
			}
			continue
		}
		itemPath := filepath.Join(source, item.Name())
		destFile := filepath.Join(dest, item.Name())
		CopyFile(itemPath, destFile)
	}
	return nil
}
