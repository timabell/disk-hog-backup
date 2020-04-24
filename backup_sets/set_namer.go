package backup_sets

import (
	"fmt"
	"regexp"
	"time"
)

const prefix = "dhb-set"

func GenerateName(getTime func() time.Time) string {
	time := getTime()
	return fmt.Sprintf("%s-%04d%02d%02d-%02d%02d%02d", prefix,
		time.Year(), time.Month(), time.Day(),
		time.Hour(), time.Minute(), time.Second())
}

func IsBackupSetName(name string) bool {
	const pattern = "^" + prefix + "-[0-9]{8}-[0-9]{6}$"
	match, err := regexp.MatchString(pattern, name)
	if err != nil {
		panic(err)
	}
	//log.Printf("pattern %s match for %s is %t", pattern, name, match)
	return match
}
