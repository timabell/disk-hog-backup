package dhcopy

import (
	"github.com/stretchr/testify/assert"
	"github.com/timabell/disk-hog-backup/test_helpers"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
	"testing"
)

const emptyFolder = "NothingInHere"

func TestCopyTree(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	dest := test_helpers.CreateTmpFolder("backups")
	defer os.RemoveAll(dest)

}

func TestCopyEmptyFolder(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	dest := test_helpers.CreateTmpFolder("backups")
	defer os.RemoveAll(dest)

	CopyFolder(source, dest)

	checkEmptyFolderCopied(t, dest)
}

func checkEmptyFolderCopied(t *testing.T, dest string) {
	dirPath := filepath.Join(dest, emptyFolder)
	dir, err := ioutil.ReadDir(dirPath)
	assert.NoError(t, err, "empty folder should be copied")
	assert.Equal(t, 0, len(dir), "empty folder in source should be empty in backup")
}

func createSource() (source string) {
	source = test_helpers.CreateTmpFolder("orig")

	const theText = "backmeup susie"
	testFileName := filepath.Join(source, "testfile.txt")
	contents := []byte(theText)
	if err := ioutil.WriteFile(testFileName, contents, 0666); err != nil {
		log.Fatal(err)
	}

	emptyFolderPath := filepath.Join(source, emptyFolder)
	os.Mkdir(emptyFolderPath, 0666)

	return source
}

