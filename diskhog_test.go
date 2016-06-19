package main

import (
	"github.com/stretchr/testify/assert"
	"testing"
)

func TestThings(t *testing.T) {
	x := Pony("rr")
	assert.Equal(t, x, "rr", "should be intact")
}
