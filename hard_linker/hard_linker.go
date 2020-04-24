package hard_linker

import (
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
)

func HardLinkCopy(source string, dest string) error {
	log.Printf("hard-linking to %v in %v\n", source, dest)
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
			if err := HardLinkCopy(dirPath, destFolder); err != nil {
				return err
			}
			continue
		}
		itemPath := filepath.Join(source, item.Name())
		destFile := filepath.Join(dest, item.Name())
		err = os.Link(itemPath, destFile)
		if err != nil {
			return err
		}
	}
	return nil
}
