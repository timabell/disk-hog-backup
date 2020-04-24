package backup_sets

import (
	"github.com/stretchr/testify/assert"
	"github.com/timabell/disk-hog-backup/test_helpers"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"
)

const backupFolderName = "backups"

func TestCreation(t *testing.T) {
	// arrange
	dest := test_helpers.CreateTmpFolder(backupFolderName)
	defer os.RemoveAll(dest)
	timeFixer := test_helpers.TimeFixer()
	expectedSetName := GenerateName(timeFixer) // figure out the generated set name, don't want to add DI mess to method signatures to inject it

	// act
	actualSetName, err := CreateEmptySet(dest, timeFixer)
	assert.NoError(t, err)
	assert.Equal(t, expectedSetName, actualSetName)

	// assert
	dirPath := filepath.Join(dest, actualSetName)
	_, err = ioutil.ReadDir(dirPath)
	assert.NoError(t, err, "set folder should be copied")
}
