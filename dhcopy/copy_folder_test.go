package dhcopy

import (
	"github.com/stretchr/testify/assert"
	"github.com/timabell/disk-hog-backup/test_helpers"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"
)

const emptyFolder = "NothingInHere"
const backupFolderName = "backups"
const deepPath = "another/level"

func TestCopiesFiles(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
	const filename = "testfile.txt"
	filePath := filepath.Join(source, filename)
	test_helpers.MakeTestFile(filePath, "backmeup susie")
	folderPath := filepath.Join(source, deepPath)
	if err := os.MkdirAll(folderPath, os.ModePerm); err != nil {
		panic(err)
	}
	const filename2 = "testfile2.txt"
	deepFilePath := filepath.Join(source, deepPath, filename2)
	test_helpers.MakeTestFile(deepFilePath, "aloha")
	dest := test_helpers.CreateTmpFolder(backupFolderName)
	defer os.RemoveAll(dest)

	CopyFolder(source, dest)

	// Just a quick check that recursion is including files.
	// Full testing of files is is in the file copier tests.
	_, err := os.Stat(filepath.Join(dest, filename))
	assert.NoError(t, err)
	_, err = os.Stat(filepath.Join(dest, deepPath, filename2))
	assert.NoError(t, err)
}

func TestCopyEmptyFolder(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)

	emptyFolderPath := filepath.Join(source, emptyFolder)
	if err := os.MkdirAll(emptyFolderPath, os.ModePerm); err != nil {
		panic(err)
	}

	dest := test_helpers.CreateTmpFolder(backupFolderName)
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
	return source
}
