package main

import (
	"github.com/timabell/disk-hog-backup/test_helpers"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
	"testing"
)

func TestSmokeBackupRealisticTree(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	dest := test_helpers.CreateTmpFolder("backups")
	defer os.RemoveAll(dest)

	// smoke test
	Backup(source, dest)
	// todo, some simple assertions
}

func TestBackupSingleFile(t *testing.T){
	t.Skip("todo")
}

func TestBackupEmptyFolder(t *testing.T){
	t.Skip("todo")
}

func TestBackupNonExistentPath(t *testing.T){
	t.Skip("todo")
}

func createSource() (source string) {
	/// todo: build a more complex source
	source = test_helpers.CreateTmpFolder("orig")

	testFileName := filepath.Join(source, "testfile.txt")
	const theText = "backmeup susie"
	contents := []byte(theText)
	if err := ioutil.WriteFile(testFileName, contents, 0666); err != nil {
		log.Fatal(err)
	}

	const emptyFolder = "NothingInHere"
	emptyFolderPath := filepath.Join(source, emptyFolder)
	os.Mkdir(emptyFolderPath, 0666)

	return source
}

