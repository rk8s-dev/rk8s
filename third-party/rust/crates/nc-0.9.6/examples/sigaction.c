
#include <stdio.h>
#include <signal.h>
#include <time.h>
#include <unistd.h>
#include <string.h>
#include <sys/signal.h>

static void signal_handler(int sig_num) {
  printf("signal handle %d\n", sig_num);
}

static const int kSigLen = 14;

int main() {
  const int kSignals[] = {
      SIGHUP,
      SIGINT,
      SIGQUIT,
      SIGILL,
      SIGABRT,
      SIGBUS,
      SIGFPE,
      SIGUSR1,
      SIGUSR2,
      SIGPIPE,
      SIGALRM,
      SIGTERM,
      SIGCHLD,
      SIGPOLL,
  };

  struct sigaction sa;
  bzero(&sa, sizeof(struct sigaction));
  sa.sa_handler = signal_handler;
  sa.sa_flags = SA_RESTART;

  for (int i = 0; i < kSigLen; ++i) {
      int sig_num = kSignals[i];
      int ret = sigaction(sig_num, &sa, NULL);
      printf("register signal handler for %d\n", sig_num);
    }

    const int pid = getpid();
    printf("pid: %d\n", pid);

    while (1) {
      struct timespec ts = { .tv_sec = 10, .tv_nsec = 0};
      int ret = nanosleep(&ts, NULL);
    }

  return 0;
}
