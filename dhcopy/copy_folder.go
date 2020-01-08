package dhcopy

import (
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
)

func CopyFolder(source string, dest string) {
	log.Printf("backing up folder %v into %v\n", source, dest)
	contents, err := ioutil.ReadDir(source)
	if err != nil {
		log.Fatal(err)
	}

	for _, item := range contents {
		//itemPath := filepath.Join(source, item.Name())
		if item.IsDir() {
			destFolder := filepath.Join(dest, item.Name())
			err := os.Mkdir(destFolder, 0666)
			if err != nil {
				log.Fatal(err)
			}
			//CopyFolder(item, dest)
			continue
		}
		//destFile := filepath.Join(dest, item.Name())
		//CopyFile(itemPath, destFile)
	}
}
