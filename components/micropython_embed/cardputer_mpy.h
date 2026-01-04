#pragma once

#include <stddef.h>
#include <stdint.h>

int cardputer_mpy_start(const char *path, size_t heap_size);
int cardputer_mpy_start_mpy(const char *path, size_t heap_size);
int cardputer_mpy_tick(uint32_t dt_ms, int32_t key_code, int32_t key_event, uint16_t *framebuffer);
void cardputer_mpy_stop(void);
const char *cardputer_mpy_last_error(void);
void cardputer_host_set_i2s_write_callback(void (*callback)(const uint8_t *, size_t));
