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

	sourceFileName := filepath.Join(sourceFolder, theFile)
	contents := []byte(theText)
	if err := ioutil.WriteFile(sourceFileName, contents, 0666); err != nil {
		log.Fatal(err)
	}

	destFileName := filepath.Join(dest, theFile)

	CopyFile(sourceFileName, destFileName)

	checkFileCopied(t, dest)
}

func checkFileCopied(t *testing.T, dest string) {
	destFileName := filepath.Join(dest, theFile)
	backupContents, err := ioutil.ReadFile(destFileName)
	assert.NoError(t, err, "failed to read file from backup folder")
	backedUpString := string(backupContents)
	assert.Equal(t, theText, backedUpString, "file contents should be copied to backup folder")
}
