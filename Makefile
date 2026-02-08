PREFIX ?= /usr/local

drastic-idle:
	cargo build --release

install: drastic-idle
	install -Dm755 target/release/drastic-idle "$(DESTDIR)$(PREFIX)/bin/drastic-idle"

clean:
	cargo clean
	rm -f drastic-idle

.PHONY: install clean
