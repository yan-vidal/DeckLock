/* Delay only the locker's main-thread XPending after the private test marker.
 * This models GTK frame work without delaying the observer's X connection.
 * Never loaded by production or by the intruder/window manager. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdlib.h>
#include <unistd.h>

static pthread_t main_thread;
static pthread_once_t resolve_once = PTHREAD_ONCE_INIT;
static int (*real_pending)(void *);

static void resolve_pending(void) {
    real_pending = dlsym(RTLD_NEXT, "XPending");
}

__attribute__((constructor)) static void remember_main_thread(void) {
    main_thread = pthread_self();
}

int XPending(void *display) {
    pthread_once(&resolve_once, resolve_pending);
    const char *marker = getenv("DECKLOCK_TEST_SLOW_MAIN_MARKER");
    if (marker && pthread_equal(pthread_self(), main_thread) && access(marker, F_OK) == 0) {
        int evidence = open(marker, O_WRONLY | O_APPEND);
        if (evidence >= 0) {
            (void)write(evidence, "d", 1);
            close(evidence);
        }
        usleep(150000);
    }
    return real_pending(display);
}
