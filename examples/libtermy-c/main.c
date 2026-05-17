#include "termy.h"

#include <stdio.h>

int main(void) {
  TermyFfiTerminal *terminal = NULL;
  TermyFfiSize size = termy_size_default();
  const char command[] = "printf 'hello from libtermy-c'";

  TermyFfiStatus status = termy_terminal_new(
      size,
      (const uint8_t *)command,
      sizeof(command) - 1,
      &terminal);
  if (status != TERMY_FFI_OK) {
    return 1;
  }

  TermyFfiFrame frame = {0};
  status = termy_terminal_snapshot(terminal, &frame);
  if (status == TERMY_FFI_OK) {
    printf("frame: %u cols x %u rows, %lu cells\n",
           frame.cols,
           frame.rows,
           (unsigned long)frame.cells_len);
    termy_frame_free(&frame);
  }

  termy_terminal_free(terminal);
  return status == TERMY_FFI_OK ? 0 : 1;
}
