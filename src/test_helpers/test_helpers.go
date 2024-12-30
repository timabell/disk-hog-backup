package test_helpers

import (
	"io/ioutil"
	"log"
	"time"
)

func CreateTmpFolder(prefix string) (newFolder string) {
	newFolder, err := ioutil.TempDir("", "dhb-"+prefix+"-")
	if err != nil {
		log.Fatal(err)
	}
	return newFolder
}

func FileContentsMatches(file1Path string, file2Path string) (bool, error) {
	file1Contents, err := readContents(file1Path)
	if err != nil {
		return false, err
	}
	file2Contents, err := readContents(file2Path)
	if err != nil {
		return false, err
	}
	return file1Contents == file2Contents, nil
}

func readContents(path string) (string, error) {
	contents, err := ioutil.ReadFile(path)
	if err != nil {
		return "", err
	}
	return string(contents), nil
}

// returns a function that always returns the same time
func TimeFixer() func() time.Time {
	fixedTime := time.Now()
	return func() time.Time {
		return fixedTime
	}
}
