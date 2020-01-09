package main

import (
	"github.com/timabell/disk-hog-backup/test_helpers"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"
)

func TestBackup(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	dest := test_helpers.CreateTmpFolder("backups")
	defer os.RemoveAll(dest)

	// smoke test
	//Backup(source, dest)

	//if _, err := os.Stat(filepath.Join(dest, "thats/deep/testfile.txt")); err != nil {
	//	t.Error(err)
	//}
}

func TestBackupSingleFile(t *testing.T) {
	t.Skip("todo")
}

func TestBackupEmptyFolder(t *testing.T) {
	t.Skip("todo")
}

func TestBackupNonExistentPath(t *testing.T) {
	t.Skip("todo")
}

func createSource() (source string) {
	source = test_helpers.CreateTmpFolder("orig")

	folderPath := filepath.Join(source, "thats")
	if err := os.Mkdir(folderPath, 0666); err != nil {
		panic(err)
	}
	folderPath = filepath.Join(folderPath, "deep")
	if err := os.Mkdir(folderPath, 0666); err != nil {
		panic(err)
	}

	testFileName := filepath.Join(folderPath, "testfile.txt")
	const theText = "backmeup susie"
	contents := []byte(theText)
	if err := ioutil.WriteFile(testFileName, contents, 0666); err != nil {
		panic(err)
	}

	return source
}
