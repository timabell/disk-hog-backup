package main

import (
	"testing"
)

func TestThings(t *testing.T) {
	x := Pony("rr")
	if x != "rr" {
		t.Errorf("pony didn't return rr got %v", x)
	}
}
