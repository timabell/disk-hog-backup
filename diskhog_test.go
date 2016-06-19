package main

import (
	"github.com/stretchr/testify/assert"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
	"testing"
)

func TestThings(t *testing.T) {
	x := Pony("rr")
	assert.Equal(t, x, "rr", "should be intact")
}

func TestCopyFile(t *testing.T) {
	source := createSource()
	defer os.RemoveAll(source)
}

func createSource() (source string) {
	source, err := ioutil.TempDir("", "dhb")
	if err != nil {
		log.Fatal(err)
	}
	testFileName := filepath.Join(source, "testfile.txt")
	contents := []byte("backmeup susie")
	if err := ioutil.WriteFile(testFileName, contents, 0666); err != nil {
		log.Fatal(err)
	}
	return source
}
