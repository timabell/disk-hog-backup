package main

import (
	"fmt"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
)

func main() {
}

func backup(source string, dest string) {
	fmt.Printf("backing up %v into %v\n", source, dest)
	contents, err := ioutil.ReadDir(source)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("contents %#v\n", contents)
	for _, item := range contents {
		fmt.Printf("item: %v\n", item)
		destFile := filepath.Join(dest, item.Name())
		copyFile(item, destFile)
	}
}

func copyFile(sourceFile os.FileInfo, dest string) {
	fmt.Printf("copying to : %v\n", dest)
	destFile, err := os.Create(dest)
	if err != nil {
		log.Fatal(err)
	}
	defer destFile.Close()
}
