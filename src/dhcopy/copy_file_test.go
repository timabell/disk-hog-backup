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

const theFile = "testfile.txt"
const theText = "backmeup susie"

func TestCopy(t *testing.T) {
	sourceFolder := test_helpers.CreateTmpFolder("orig")
	defer os.RemoveAll(sourceFolder)
	dest := test_helpers.CreateTmpFolder("backups")
	defer os.RemoveAll(dest)

	sourceFilePath := filepath.Join(sourceFolder, theFile)
	contents := []byte(theText)
	if err := ioutil.WriteFile(sourceFilePath, contents, 0666); err != nil {
		log.Fatal(err)
	}

	destinationFilePath := filepath.Join(dest, theFile)

	CopyFile(sourceFilePath, destinationFilePath)

	contentsMatches, err := test_helpers.FileContentsMatches(sourceFilePath, destinationFilePath)
	assert.NoError(t, err)
	assert.True(t, contentsMatches, "file contents should be copied to backup folder")
}
