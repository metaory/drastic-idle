#define _POSIX_C_SOURCE 200809L
#include <X11/Xatom.h>
#include <X11/Xlib.h>
#include <X11/extensions/scrnsaver.h>
#include <getopt.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <time.h>
#include <unistd.h>

#define TIMER_W 220
#define TIMER_H 28
#define TIMER_MARGIN 10

typedef struct {
	unsigned long phase1_ms, phase2_ms, auto_snooze_ms;
	unsigned int poll_s;
} config_t;

#define DEFAULT_PHASE1_MS 10000
#define DEFAULT_PHASE2_MS 300000
#define DEFAULT_AUTO_SNOOZE_MS 60000
#define DEFAULT_POLL_S 2

static unsigned long now_ms(void) {
	struct timespec ts;
	clock_gettime(CLOCK_MONOTONIC, &ts);
	return (unsigned long)ts.tv_sec * 1000 + (unsigned long)ts.tv_nsec / 1000000;
}

static unsigned long get_idle_ms(Display *d) {
	XScreenSaverInfo *info = XScreenSaverAllocInfo();
	if (!info) return 0;
	unsigned long ms = 0;
	if (XScreenSaverQueryInfo(d, DefaultRootWindow(d), info))
		ms = (unsigned long)info->idle;
	XFree(info);
	return ms;
}

static void close_active_window(Display *d) {
	Atom a = XInternAtom(d, "_NET_ACTIVE_WINDOW", False);
	if (a == None) return;
	Atom type;
	int format;
	unsigned long nitems, after;
	unsigned char *data = NULL;
	if (XGetWindowProperty(d, DefaultRootWindow(d), a, 0, 1, False, XA_WINDOW,
		&type, &format, &nitems, &after, &data) != Success || !data || nitems == 0)
		return;
	Window win = *(Window *)data;
	XFree(data);
	if (win) XKillClient(d, win);
}

static void format_timer(char *buf, size_t size, const config_t *c, unsigned long idle,
		unsigned long snoozed_until, int phase1_done, unsigned long now) {
	unsigned long rem_ms = (snoozed_until > now) ? (snoozed_until - now) : 0;
	if (rem_ms) {
		unsigned long rem_s = rem_ms / 1000;
		snprintf(buf, size, "Snoozed %lu:%02lu.%lu", rem_s / 60, rem_s % 60, (rem_ms % 1000) / 100);
		return;
	}
	unsigned long idle_s = idle / 1000;
	unsigned long idle_t = (idle % 1000) / 100;
	unsigned long p2_ms = idle >= c->phase2_ms ? 0 : c->phase2_ms - idle;
	unsigned long p2_sec = p2_ms / 1000;
	unsigned long p2_t = (p2_ms % 1000) / 100;
	if (phase1_done || idle >= c->phase1_ms) {
		snprintf(buf, size, "Idle %lu:%02lu.%lu P1 %s Off %lu:%02lu.%lu",
			idle_s / 60, idle_s % 60, idle_t, phase1_done ? "done" : "0s",
			p2_sec / 60, p2_sec % 60, p2_t);
		return;
	}
	unsigned long p1_ms = c->phase1_ms - idle;
	unsigned long p1_s = p1_ms / 1000;
	unsigned long p1_t = (p1_ms % 1000) / 100;
	snprintf(buf, size, "Idle %lu:%02lu.%lu P1 %lu.%lus Off %lu:%02lu.%lu",
		idle_s / 60, idle_s % 60, idle_t, p1_s, p1_t, p2_sec / 60, p2_sec % 60, p2_t);
}

static void draw_timer(Display *d, Window win, GC gc, const config_t *c, unsigned long idle,
		unsigned long snoozed_until, int phase1_done, unsigned long now) {
	char buf[96];
	format_timer(buf, sizeof buf, c, idle, snoozed_until, phase1_done, now);
	XClearWindow(d, win);
	XDrawString(d, win, gc, 5, 18, buf, (int)strlen(buf));
	XFlush(d);
}

static void process_events(Display *d) {
	while (XPending(d)) {
		XEvent ev;
		XNextEvent(d, &ev);
	}
}

/** Returns 1 if process should exit (poweroff), 0 to continue. */
static int run_phases(Display *d, const config_t *c, unsigned long idle, unsigned long now,
		int *phase1_done, unsigned long *snoozed_until) {
	if (idle < c->phase1_ms) {
		*phase1_done = 0;
		*snoozed_until = 0;
	}
	if (now < *snoozed_until) return 0;
	if (idle >= c->phase2_ms) return 1;
	if (idle >= c->phase1_ms && !*phase1_done) {
		close_active_window(d);
		*phase1_done = 1;
		*snoozed_until = now + c->auto_snooze_ms;
	}
	return 0;
}

static void wait_poll(int xfd, unsigned int poll_s) {
	fd_set fds;
	struct timeval tv = { (time_t)poll_s, 0 };
	FD_ZERO(&fds);
	FD_SET(xfd, &fds);
	select(xfd + 1, &fds, NULL, NULL, &tv);
}

static void usage(const char *name) {
	fprintf(stderr,
		"usage: %s [options]\n"
		"  --phase1 SEC      idle before close window (default 10)\n"
		"  --phase2 SEC      idle before poweroff (default 300)\n"
		"  --auto-snooze SEC snooze after phase1 (default 60)\n"
		"  --poll SEC        poll interval (default 2)\n"
		"  -h, --help        show this\n",
		name);
}

static int parse_args(int argc, char **argv, config_t *c) {
	c->phase1_ms = DEFAULT_PHASE1_MS;
	c->phase2_ms = DEFAULT_PHASE2_MS;
	c->auto_snooze_ms = DEFAULT_AUTO_SNOOZE_MS;
	c->poll_s = DEFAULT_POLL_S;
	static struct option opts[] = {
		{ "phase1",       required_argument, 0, '1' },
		{ "phase2",       required_argument, 0, '2' },
		{ "auto-snooze",  required_argument, 0, 'a' },
		{ "poll",         required_argument, 0, 'p' },
		{ "help",         no_argument,       0, 'h' },
		{ 0, 0, 0, 0 }
	};
	int opt;
	while ((opt = getopt_long(argc, argv, "1:2:a:p:h", opts, NULL)) != -1) {
		unsigned long val;
		char *end;
		switch (opt) {
		case '1': val = strtoul(optarg, &end, 10); if (*end || val == 0) goto bad; c->phase1_ms = val * 1000; break;
		case '2': val = strtoul(optarg, &end, 10); if (*end || val == 0) goto bad; c->phase2_ms = val * 1000; break;
		case 'a': val = strtoul(optarg, &end, 10); if (*end) goto bad; c->auto_snooze_ms = val * 1000; break;
		case 'p': val = strtoul(optarg, &end, 10); if (*end || val == 0) goto bad; c->poll_s = (unsigned int)val; break;
		case 'h': usage(argv[0]); exit(0);
		default: usage(argv[0]); return -1;
		}
		continue;
	bad:
		fprintf(stderr, "%s: invalid value for option: %s\n", argv[0], optarg);
		return -1;
	}
	if (c->phase1_ms >= c->phase2_ms) {
		fprintf(stderr, "%s: phase1 must be less than phase2\n", argv[0]);
		return -1;
	}
	return 0;
}

int main(int argc, char **argv) {
	config_t cfg;
	if (parse_args(argc, argv, &cfg) != 0) return 1;
	Display *d = XOpenDisplay(NULL);
	if (!d) return 1;
	int screen = DefaultScreen(d);
	Window root = DefaultRootWindow(d);
	int tw_x = DisplayWidth(d, screen) - TIMER_W - TIMER_MARGIN;
	XSetWindowAttributes attrs = { .override_redirect = True,
		.background_pixel = WhitePixel(d, screen),
		.border_pixel = BlackPixel(d, screen) };
	Window timer_win = XCreateWindow(d, root, tw_x, TIMER_MARGIN, TIMER_W, TIMER_H,
		1, CopyFromParent, CopyFromParent, CopyFromParent,
		CWOverrideRedirect | CWBackPixel | CWBorderPixel, &attrs);
	GC gc = XCreateGC(d, timer_win, 0, NULL);
	XFontStruct *font = XLoadQueryFont(d, "fixed");
	if (font) XSetFont(d, gc, font->fid);
	XSetForeground(d, gc, BlackPixel(d, screen));
	XMapWindow(d, timer_win);
	int phase1_done = 0;
	unsigned long snoozed_until = 0;
	int xfd = ConnectionNumber(d);
	for (;;) {
		wait_poll(xfd, cfg.poll_s);
		process_events(d);
		unsigned long idle = get_idle_ms(d);
		unsigned long now = now_ms();
		if (run_phases(d, &cfg, idle, now, &phase1_done, &snoozed_until)) {
			XCloseDisplay(d);
			execlp("systemctl", "systemctl", "poweroff", (char *)NULL);
			return 1;
		}
		draw_timer(d, timer_win, gc, &cfg, idle, snoozed_until, phase1_done, now);
	}
}
