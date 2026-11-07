.PHONY: run build release check fmt install

run:
	cargo run -- --root "$${APKEZ_ROOT:-.}"

build:
	cargo build

release:
	cargo build --release

check:
	cargo check --offline

fmt:
	cargo fmt --all

install: release
	install -Dm755 target/release/apkez "$${DESTDIR:-}$${PREFIX:-$$HOME/.local}/bin/apkez"
