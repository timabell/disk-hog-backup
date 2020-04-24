package backup_sets

import (
	"github.com/stretchr/testify/assert"
	"github.com/timabell/disk-hog-backup/test_helpers"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"
	"time"
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

func TestFindLatestSet(t *testing.T) {
	// arrange
	dest := test_helpers.CreateTmpFolder(backupFolderName)
	//defer os.RemoveAll(dest)

	// create out of order to differentiate creation order form file name order
	baseDate := time.Date(2019, 12, 31, 23, 59, 0, 0, time.UTC)
	_, err := CreateEmptySet(dest,
		test_helpers.FixedTime(baseDate.Add(time.Second)))
	assert.NoError(t, err)
	expectedSetName, err := CreateEmptySet(dest,
		test_helpers.FixedTime(baseDate.Add(time.Second*3)))
	assert.NoError(t, err)
	_, err = CreateEmptySet(dest,
		test_helpers.FixedTime(baseDate.Add(time.Second*2)))
	assert.NoError(t, err)

	// act
	actualSetName, err := FindLatestSet(dest)

	// assert
	assert.NoError(t, err)
	assert.Equal(t, expectedSetName, actualSetName)
}

func TestFindLatestSet_WhenNoSets(t *testing.T) {
	// arrange
	dest := test_helpers.CreateTmpFolder(backupFolderName)
	defer os.RemoveAll(dest)

	// act
	actualSetName, err := FindLatestSet(dest)

	// assert
	assert.NoError(t, err)
	assert.Equal(t, "", actualSetName)
}

func TestFindLatestSet_IgnoresOtherFolders(t *testing.T) {
	// arrange
	dest := test_helpers.CreateTmpFolder(backupFolderName)
	defer os.RemoveAll(dest)

	folderPath := filepath.Join(dest, "this-is-not-a-backup-set")
	err := os.MkdirAll(folderPath, os.ModePerm)
	assert.NoError(t, err)

	// act
	actualSetName, err := FindLatestSet(dest)

	// assert
	assert.NoError(t, err)
	assert.Equal(t, "", actualSetName)
}

func TestIsBackupSetName(t *testing.T) {
	testIsBackupSetName(t, true, "dhb-set-19981231-095958")
	testIsBackupSetName(t, false, "dhb-set-19981231-0959588")
	testIsBackupSetName(t, false, "something-else-19981231-095958")
	testIsBackupSetName(t, false, "dhb-set-19981231-095958-old")
	testIsBackupSetName(t, false, "not-a-dhb-set-19981231-095958")
}

func testIsBackupSetName(t *testing.T, expected bool, name string) bool {
	t.Logf("IsBackupSetName(\"%s\") should be %t", name, expected)
	return assert.Equal(t, expected, IsBackupSetName(name))
}
