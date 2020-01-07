package dhcopy

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

func TestCopy(t *testing.T) {
	sourceFolder := createTmpFolder("orig")
	defer os.RemoveAll(sourceFolder)
	dest := createTmpFolder("backups")
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

func createTmpFolder(prefix string) (newFolder string) {
	newFolder, err := ioutil.TempDir("", "dhb-"+prefix+"-")
	if err != nil {
		log.Fatal(err)
	}
	return newFolder
}
