package backup_sets

import (
	"github.com/stretchr/testify/assert"
	"testing"
	"time"
)

func TestGeneratesSetName(t *testing.T) {
	fixedTime := time.Date(2001, 2, 3, 14, 5, 6, 7, time.UTC)
	name := GenerateName(func() time.Time {
		return fixedTime
	})
	assert.Equal(t, "dhb-set-20010203-140506", name)
}
