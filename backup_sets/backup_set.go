package backup_sets

import (
	"io/ioutil"
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

func FindLatestSet(dest string) (setName string, err error) {
	contents, err := ioutil.ReadDir(dest)
	if err != nil {
		return "", err
	}
	backupSets := filterDir(contents, func(info os.FileInfo) bool {
		return IsBackupSetName(info.Name())
	})
	if len(backupSets) < 1 {
		return "", nil
	}
	return backupSets[len(backupSets)-1].Name(), nil
}

func filterDir(list []os.FileInfo, f func(info os.FileInfo) bool) (results []os.FileInfo) {
	for _, item := range list {
		if f(item) {
			results = append(results, item)
		}
	}
	return results
}
