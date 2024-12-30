package backup_sets

import (
	"fmt"
	"time"
)

func GenerateName(getTime func() time.Time) string {
	time := getTime()
	return fmt.Sprintf("dhb-set-%04d%02d%02d-%02d%02d%02d",
		time.Year(), time.Month(), time.Day(),
		time.Hour(), time.Minute(), time.Second())
}
