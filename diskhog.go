package main

import (
	"fmt"
	"io/ioutil"
	"log"
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
}
