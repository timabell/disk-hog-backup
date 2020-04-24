package backup_sets

import (
	"os"
	"path/filepath"
	"time"
)

func CreateEmptySet(dest string, getTime func() time.Time) (setName string, err error) {
	setName = GenerateName(getTime)
	destFolder := filepath.Join(dest, setName)
	err = os.MkdirAll(destFolder, os.ModePerm)
	return
}
