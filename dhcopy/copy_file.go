package dhcopy

import (
	"fmt"
	"io"
	"log"
	"os"
)

func CopyFile(source string, dest string) {
	fmt.Printf("copying %v to : %v\n", source, dest)

	srcFile, err := os.Open(source)
	if err != nil {
		log.Fatal(err)
	}
	defer srcFile.Close()

	destFile, err := os.Create(dest)
	if err != nil {
		log.Fatal(err)
	}
	defer destFile.Close()

	bytesWritten, err := io.Copy(destFile, srcFile)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("%v bytes copied\n", bytesWritten)
}
