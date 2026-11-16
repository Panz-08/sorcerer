# Convenience targets for Sorcerer.
#
#   make            build release
#   make test       run the test suite
#   make install    install to $(PREFIX)/bin (default ~/.local/bin)
#   make check      clippy with warnings denied

CARGO ?= cargo
PREFIX ?= $(HOME)/.local
DESTDIR ?=
BIN := target/release/sorce

.PHONY: all build install test check fmt run clean

all: build

build:
	$(CARGO) build --release

install: build
	install -d "$(DESTDIR)$(PREFIX)/bin"
	install -m 755 "$(BIN)" "$(DESTDIR)$(PREFIX)/bin/sorce"
	@echo "Installed $(DESTDIR)$(PREFIX)/bin/sorce"

test:
	$(CARGO) test

check:
	$(CARGO) clippy --all-targets -- -D warnings

fmt:
	$(CARGO) fmt

run:
	$(CARGO) run

clean:
	$(CARGO) clean
