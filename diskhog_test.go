package main

import (
	"github.com/stretchr/testify/assert"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
	"testing"
)

const theFile = "testfile.txt"
const theText = "backmeup susie"

func TestCopyFile(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	dest := createTmpFolder()
	defer os.RemoveAll(dest)

	backup(source, dest)

	destFileName := filepath.Join(dest, theFile)
	backupContents, err := ioutil.ReadFile(destFileName)
	if err != nil {
		log.Fatal(err)
	}
	backedUpString := string(backupContents)
	assert.Equal(t, theText, backedUpString, "file contents should be copied to backup folder")
}

func createSource() (source string) {
	source = createTmpFolder()
	testFileName := filepath.Join(source, "testfile.txt")
	contents := []byte(theText)
	if err := ioutil.WriteFile(testFileName, contents, 0666); err != nil {
		log.Fatal(err)
	}
	return source
}

func createTmpFolder() (newFolder string) {
	newFolder, err := ioutil.TempDir("", "dhb")
	if err != nil {
		log.Fatal(err)
	}
	return newFolder
}
