package test_helpers

import (
	"io/ioutil"
	"log"
)

func CreateTmpFolder(prefix string) (newFolder string) {
	newFolder, err := ioutil.TempDir("", "dhb-"+prefix+"-")
	if err != nil {
		log.Fatal(err)
	}
	return newFolder
}
