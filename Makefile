.PHONY: all build run test clean

all: build

build:
	cargo xtask build

run:
	cargo xtask run

test:
	cargo xtask test

clean:
	cargo clean
