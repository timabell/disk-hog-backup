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
const emptyFolder = "NothingInHere"

func TestCopy(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	dest := createTmpFolder("backups")
	defer os.RemoveAll(dest)

	Backup(source, dest)

	checkFileCopied(t, dest)
	checkEmptyFolderCopied(t, dest)
}

func checkFileCopied(t *testing.T, dest string) {
	destFileName := filepath.Join(dest, theFile)
	backupContents, err := ioutil.ReadFile(destFileName)
	assert.NoError(t, err, "failed to read file from backup folder")
	backedUpString := string(backupContents)
	assert.Equal(t, theText, backedUpString, "file contents should be copied to backup folder")
}

func checkEmptyFolderCopied(t *testing.T, dest string) {
	dirPath := filepath.Join(dest, emptyFolder)
	dir, err := ioutil.ReadDir(dirPath)
	assert.NoError(t, err, "empty folder should be copied")
	assert.Equal(t, 0, len(dir), "empty folder in source should be empty in backup")
}

func createSource() (source string) {
	source = createTmpFolder("orig")

	testFileName := filepath.Join(source, "testfile.txt")
	contents := []byte(theText)
	if err := ioutil.WriteFile(testFileName, contents, 0666); err != nil {
		log.Fatal(err)
	}

	emptyFolderPath := filepath.Join(source, emptyFolder)
	os.Mkdir(emptyFolderPath, 0666)

	return source
}

func createTmpFolder(prefix string) (newFolder string) {
	newFolder, err := ioutil.TempDir("", "dhb-"+prefix+"-")
	if err != nil {
		log.Fatal(err)
	}
	return newFolder
}
