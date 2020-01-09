#!/bin/sh
go clean -testcache
# $@ relays args to script so you can run ./test.sh -v to see skipped tests etc
go test ./... $@
