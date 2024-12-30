package backup_sets

import (
	"github.com/stretchr/testify/assert"
	"github.com/timabell/disk-hog-backup/test_helpers"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"
)

const deepPath = "thats/deep"
const backupFolderName = "backups"

func TestBackup(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	dest := test_helpers.CreateTmpFolder("backups")
	defer os.RemoveAll(dest) // comment this out to be able to inspect what we actually got

	//smoke test
	setName, err := Backup(source, dest)
	assert.NoError(t, err)

	// Just a quick check that deeply nested file is copied.
	// All other edge cases are tested in unit tests.
	_, err = os.Stat(filepath.Join(dest, setName, deepPath,"/testfile.txt"))
	assert.NoError(t, err)
}

func TestBackupNonExistentPath(t *testing.T) {
	t.Skip("todo")
}

func TestCreatesDestinationFolder(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	dest := test_helpers.CreateTmpFolder(backupFolderName)
	defer os.RemoveAll(dest)

	nonExistentDestination := filepath.Join(dest, "to-be-created")

	Backup(source, nonExistentDestination)

	_, err := ioutil.ReadDir(nonExistentDestination)
	assert.NoError(t, err, "destination folder should be copied")
}

func createSource() (source string) {
	source = test_helpers.CreateTmpFolder("orig")

	folderPath := filepath.Join(source, deepPath)
	if err := os.MkdirAll(folderPath, os.ModePerm); err != nil {
		panic(err)
	}

	testFileName := filepath.Join(folderPath, "testfile.txt")
	const theText = "backmeup susie"
	contents := []byte(theText)
	if err := ioutil.WriteFile(testFileName, contents, os.ModePerm); err != nil {
		panic(err)
	}

	return source
}
