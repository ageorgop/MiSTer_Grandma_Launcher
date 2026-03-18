TARGET := armv7-unknown-linux-musleabihf
HOST ?= mister

.PHONY: check test build deploy clean

check:
	cargo check

test:
	cargo test

build:
	cross build --release --target $(TARGET)

deploy:
	./deploy.sh $(HOST)

clean:
	cargo clean
