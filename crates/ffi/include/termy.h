#ifndef TERMY_H
#define TERMY_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
  TERMY_FFI_OK = 0,
  TERMY_FFI_NULL = 1,
  TERMY_FFI_INVALID_UTF8 = 2,
  TERMY_FFI_SPAWN_FAILED = 3,
  TERMY_FFI_CONFIG_LOAD_FAILED = 4,
} TermyFfiStatus;

typedef enum {
  TERMY_FFI_EVENT_WAKEUP = 1,
  TERMY_FFI_EVENT_TITLE = 2,
  TERMY_FFI_EVENT_RESET_TITLE = 3,
  TERMY_FFI_EVENT_BELL = 4,
  TERMY_FFI_EVENT_EXIT = 5,
  TERMY_FFI_EVENT_CLIPBOARD_STORE = 6,
  TERMY_FFI_EVENT_SHELL_PROMPT_START = 7,
  TERMY_FFI_EVENT_SHELL_COMMAND_START = 8,
  TERMY_FFI_EVENT_SHELL_COMMAND_EXECUTING = 9,
  TERMY_FFI_EVENT_SHELL_COMMAND_FINISHED = 10,
  TERMY_FFI_EVENT_PROGRESS = 11,
  TERMY_FFI_EVENT_WORKING_DIRECTORY = 12,
} TermyFfiEventKind;

typedef enum {
  TERMY_FFI_PROGRESS_CLEAR = 0,
  TERMY_FFI_PROGRESS_IN_PROGRESS = 1,
  TERMY_FFI_PROGRESS_ERROR = 2,
  TERMY_FFI_PROGRESS_INDETERMINATE = 3,
  TERMY_FFI_PROGRESS_WARNING = 4,
} TermyFfiProgressState;

typedef struct TermyFfiTerminal TermyFfiTerminal;
typedef struct TermyFfiConfig TermyFfiConfig;

typedef struct {
  uint16_t cols;
  uint16_t rows;
  float cell_width;
  float cell_height;
} TermyFfiSize;

typedef struct {
  uint8_t r;
  uint8_t g;
  uint8_t b;
  uint8_t a;
} TermyFfiColor;

typedef struct {
  size_t col;
  size_t row;
  uint32_t codepoint;
  TermyFfiColor fg;
  TermyFfiColor bg;
  bool uses_terminal_default_bg;
  bool bold;
  bool render_text;
} TermyFfiCell;

typedef struct {
  bool visible;
  size_t col;
  size_t row;
  uint32_t style;
} TermyFfiCursor;

typedef struct {
  uint16_t cols;
  uint16_t rows;
  TermyFfiCell *cells_ptr;
  size_t cells_len;
  size_t cells_capacity;
  TermyFfiCursor cursor;
  size_t display_offset;
  size_t history_size;
} TermyFfiFrame;

typedef struct {
  uint8_t *ptr;
  size_t len;
  size_t capacity;
} TermyFfiBytes;

typedef struct {
  uint32_t kind;
  int32_t exit_code;
  uint8_t progress_state;
  uint8_t progress_value;
  TermyFfiBytes payload;
} TermyFfiEvent;

typedef struct {
  TermyFfiEvent *events_ptr;
  size_t events_len;
  size_t events_capacity;
  bool has_more;
} TermyFfiEventBatch;

typedef struct {
  size_t row;
  size_t left_col;
  size_t right_col;
} TermyFfiDirtySpan;

typedef struct {
  uint32_t kind;
  TermyFfiDirtySpan *spans_ptr;
  size_t spans_len;
  size_t spans_capacity;
} TermyFfiDamage;

typedef struct {
  size_t row;
  size_t start_col;
  size_t end_col;
  TermyFfiBytes line;
} TermyFfiSearchMatch;

typedef struct {
  TermyFfiSearchMatch *matches_ptr;
  size_t matches_len;
  size_t matches_capacity;
} TermyFfiSearchBatch;

typedef struct {
  size_t line_number;
  uint32_t kind;
  TermyFfiBytes message;
} TermyFfiConfigDiagnostic;

typedef struct {
  TermyFfiConfigDiagnostic *diagnostics_ptr;
  size_t diagnostics_len;
  size_t diagnostics_capacity;
} TermyFfiConfigDiagnosticBatch;

typedef struct {
  TermyFfiBytes font_family;
  TermyFfiBytes active_theme;
  TermyFfiColor foreground;
  TermyFfiColor background;
  TermyFfiColor cursor;
  float font_size;
  float line_height;
  float padding_x;
  float padding_y;
  float background_opacity;
  bool background_opacity_cells;
  bool cursor_blink;
  uint32_t cursor_style;
  float cell_width;
  float cell_height;
} TermyFfiRenderConfig;

typedef struct {
  const uint8_t *key_ptr;
  size_t key_len;
  const uint8_t *value_ptr;
  size_t value_len;
} TermyFfiEnvVar;

typedef struct {
  const TermyFfiConfig *config;
  const uint8_t *working_directory_ptr;
  size_t working_directory_len;
  const uint8_t *startup_command_ptr;
  size_t startup_command_len;
  const TermyFfiEnvVar *env_vars_ptr;
  size_t env_vars_len;
} TermyFfiTerminalOptions;

typedef struct {
  bool control;
  bool alt;
  bool shift;
  bool platform;
  bool function;
  const uint8_t *key_ptr;
  size_t key_len;
  const uint8_t *key_char_ptr;
  size_t key_char_len;
  uint32_t event_kind;
} TermyFfiKeystroke;

TermyFfiSize termy_size_default(void);
TermyFfiStatus termy_terminal_new(
    TermyFfiSize size,
    const uint8_t *startup_command_ptr,
    size_t startup_command_len,
    TermyFfiTerminal **out_terminal);
TermyFfiStatus termy_terminal_new_with_config(
    TermyFfiSize size,
    const TermyFfiConfig *config,
    const uint8_t *startup_command_ptr,
    size_t startup_command_len,
    TermyFfiTerminal **out_terminal);
TermyFfiStatus termy_terminal_new_with_options(
    TermyFfiSize size,
    const TermyFfiTerminalOptions *options,
    TermyFfiTerminal **out_terminal);
TermyFfiStatus termy_config_load_default(TermyFfiConfig **out_config);
TermyFfiStatus termy_config_load_path(
    const uint8_t *path_ptr,
    size_t path_len,
    TermyFfiConfig **out_config);
TermyFfiStatus termy_config_from_contents(
    const uint8_t *contents_ptr,
    size_t contents_len,
    TermyFfiConfig **out_config);
TermyFfiStatus termy_config_free(TermyFfiConfig *config);
bool termy_config_loaded_from_disk(const TermyFfiConfig *config);
size_t termy_config_runtime_scrollback_history(const TermyFfiConfig *config);
size_t termy_config_diagnostic_count(const TermyFfiConfig *config);
TermyFfiStatus termy_config_window_size(
    const TermyFfiConfig *config,
    float *out_width,
    float *out_height);
TermyFfiStatus termy_config_working_directory(
    const TermyFfiConfig *config,
    TermyFfiBytes *out_working_directory);
TermyFfiStatus termy_config_path(
    const TermyFfiConfig *config,
    TermyFfiBytes *out_path);
TermyFfiStatus termy_config_diagnostics(
    const TermyFfiConfig *config,
    TermyFfiConfigDiagnosticBatch *out_batch);
TermyFfiStatus termy_config_diagnostics_free(TermyFfiConfigDiagnosticBatch *batch);
TermyFfiStatus termy_config_render_config(
    const TermyFfiConfig *config,
    TermyFfiRenderConfig *out_render_config);
TermyFfiStatus termy_render_config_free(TermyFfiRenderConfig *render_config);
TermyFfiStatus termy_terminal_free(TermyFfiTerminal *terminal);
TermyFfiStatus termy_terminal_write(
    TermyFfiTerminal *terminal,
    const uint8_t *bytes_ptr,
    size_t bytes_len);
TermyFfiStatus termy_terminal_encode_key(
    TermyFfiTerminal *terminal,
    const TermyFfiKeystroke *keystroke,
    TermyFfiBytes *out_bytes);
TermyFfiStatus termy_terminal_resize(TermyFfiTerminal *terminal, TermyFfiSize size);
TermyFfiStatus termy_terminal_set_wakeup_enabled(
    TermyFfiTerminal *terminal,
    bool enabled);
TermyFfiStatus termy_terminal_scroll_display(
    TermyFfiTerminal *terminal,
    int32_t delta_lines,
    bool *out_changed);
TermyFfiStatus termy_terminal_scroll_to_bottom(
    TermyFfiTerminal *terminal,
    bool *out_changed);
TermyFfiStatus termy_terminal_snapshot(
    TermyFfiTerminal *terminal,
    TermyFfiFrame *out_frame);
TermyFfiStatus termy_frame_free(TermyFfiFrame *frame);
TermyFfiStatus termy_terminal_take_damage(
    TermyFfiTerminal *terminal,
    TermyFfiDamage *out_damage);
TermyFfiStatus termy_damage_free(TermyFfiDamage *damage);
TermyFfiStatus termy_terminal_drain_events(
    TermyFfiTerminal *terminal,
    TermyFfiEventBatch *out_batch);
TermyFfiStatus termy_event_batch_free(TermyFfiEventBatch *batch);
TermyFfiStatus termy_terminal_search(
    TermyFfiTerminal *terminal,
    const uint8_t *query_ptr,
    size_t query_len,
    TermyFfiSearchBatch *out_batch);
TermyFfiStatus termy_search_batch_free(TermyFfiSearchBatch *batch);
TermyFfiStatus termy_buffer_free(TermyFfiBytes bytes);
TermyFfiBytes termy_null_buffer(void);
size_t termy_runtime_config_default_scrollback(void);
size_t termy_terminal_options_default_scrollback(void);
TermyFfiStatus termy_query_color_default_foreground(TermyFfiColor *out_color);

#ifdef __cplusplus
}
#endif

#endif
