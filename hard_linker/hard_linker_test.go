package hard_linker

import (
	"github.com/stretchr/testify/assert"
	"github.com/timabell/disk-hog-backup/test_helpers"
	"os"
	"path/filepath"
	"testing"
)

const backupFolderName = "links"

func TestHardLinksFiles(t *testing.T) {
	source := test_helpers.CreateTmpFolder(backupFolderName + "-src")
	defer os.RemoveAll(source)
	const filename = "linkme.txt"
	test_helpers.MakeTestFile(source, filename, "hello go")
	dest := test_helpers.CreateTmpFolder(backupFolderName + "-dest")
	defer os.RemoveAll(dest)

	err := HardLinkCopy(source, dest)
	assert.NoError(t, err)

	destFile, err := os.Stat(filepath.Join(dest, filename))
	assert.NoError(t, err)
	sourceFile, err := os.Stat(filepath.Join(source, filename))
	assert.NoError(t, err)
	assert.True(t, os.SameFile(sourceFile, destFile), "files should be hard-linked (os.SameFile)")
}
