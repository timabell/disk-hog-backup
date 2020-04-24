package hard_linker

import (
	"github.com/stretchr/testify/assert"
	"github.com/timabell/disk-hog-backup/test_helpers"
	"os"
	"path/filepath"
	"testing"
)

const backupFolderName = "links"
const deepPath = "chain/reaction"

func TestHardLinksFiles(t *testing.T) {
	source := test_helpers.CreateTmpFolder(backupFolderName + "-src")
	defer os.RemoveAll(source)
	const filename = "linkme.txt"
	filePath := filepath.Join(source, filename)
	test_helpers.MakeTestFile(filePath, "hello go")
	folderPath := filepath.Join(source, deepPath)
	if err := os.MkdirAll(folderPath, os.ModePerm); err != nil {
		panic(err)
	}
	const filename2 = "linkme2.txt"
	deepFilePath := filepath.Join(source, deepPath, filename2)
	test_helpers.MakeTestFile(deepFilePath, "goodbye ruby")
	dest := test_helpers.CreateTmpFolder(backupFolderName + "-dest")
	defer os.RemoveAll(dest)

	err := HardLinkCopy(source, dest)
	assert.NoError(t, err)

	destFile, err := os.Stat(filepath.Join(dest, filename))
	assert.NoError(t, err)
	sourceFile, err := os.Stat(filepath.Join(source, filename))
	assert.NoError(t, err)
	assert.True(t, os.SameFile(sourceFile, destFile), "files should be hard-linked (os.SameFile)")

	destFile2, err := os.Stat(filepath.Join(dest, deepPath, filename2))
	assert.NoError(t, err)
	sourceFile2, err := os.Stat(filepath.Join(source, deepPath, filename2))
	assert.NoError(t, err)
	assert.True(t, os.SameFile(sourceFile2, destFile2), "files should be hard-linked (os.SameFile)")
}
