shell=/bin/bash

all: dep
	cargo build --release

dep:
ifeq (, $(shell which cargo))
	@echo "try to install Rust programming language"
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs
else
	@echo "Rust is installed"
endif

install:
	install -m 755 scripts/acpied-init /bin/
	install -m 755 scripts/acpied-apply /bin/
	install -m 755 target/release/acpied /bin/

clean:
	cargo clean
