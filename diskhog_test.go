package main

import (
	"testing"
)

func TestThings(t *testing.T) {
}

func TestFailit(t *testing.T) {
	t.Errorf("doh")
}
