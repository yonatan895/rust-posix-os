.PHONY: all build run test clean

all: build

build:
	cargo build --workspace --exclude xtask --target x86_64-rust-posix-os.json -Zbuild-std=core,compiler_builtins,alloc -Zbuild-std-features=compiler-builtins-mem

run:
	cargo run --manifest-path tools/xtask/Cargo.toml -- run

test:
	cargo run --manifest-path tools/xtask/Cargo.toml -- test

clean:
	cargo clean
