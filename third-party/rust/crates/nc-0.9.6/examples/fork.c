
#include <stdio.h>
#include <unistd.h>

int main() {
  int pid = fork();
  if (pid < 0) {
    return 1;
  } else if (pid == 0) {
    printf("[child] pid: %d\n", getpid());
    return 0;
  } else {
    printf("[parent] child pid is: %d\n", pid);
  }
  return 0;
}
