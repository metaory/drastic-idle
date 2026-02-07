PREFIX ?= /usr/local
CC ?= gcc
CFLAGS ?= -O2 -Wall

drastic-idle: drastic-idle.c
	$(CC) $(CFLAGS) -o $@ $< -lX11 -lXss

install: drastic-idle
	install -Dm755 drastic-idle "$(DESTDIR)$(PREFIX)/bin/drastic-idle"

clean:
	rm -f drastic-idle

.PHONY: install clean
