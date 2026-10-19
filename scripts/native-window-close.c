/* Send WM_DELETE_WINDOW to the already PID-verified, owned X11 test window. */
#include <X11/Xlib.h>
#include <stdlib.h>

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    char *end = NULL;
    unsigned long window = strtoul(argv[1], &end, 10);
    if (!window || !end || *end) return 2;
    Display *display = XOpenDisplay(NULL);
    if (!display) return 3;
    XEvent event = {0};
    event.xclient.type = ClientMessage;
    event.xclient.window = window;
    event.xclient.message_type = XInternAtom(display, "WM_PROTOCOLS", False);
    event.xclient.format = 32;
    event.xclient.data.l[0] = XInternAtom(display, "WM_DELETE_WINDOW", False);
    event.xclient.data.l[1] = CurrentTime;
    int sent = XSendEvent(display, window, False, NoEventMask, &event);
    XFlush(display);
    XCloseDisplay(display);
    return sent ? 0 : 4;
}
