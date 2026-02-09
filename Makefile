##
# dxwm
#
# @file
# @version 0.1

.PHONY: build install run xephyr run-xephyr clean

build:
	cargo build --release

install:
	cargo install --path .

xephyr:
	sh scripts/xephyr.sh

run-xephyr: build
	sh scripts/xephyr.sh

clean:
	cargo clean

# end
