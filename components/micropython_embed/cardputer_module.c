#include <stdbool.h>
#include <stdint.h>

#include "py/obj.h"
#include "py/runtime.h"

#ifndef STATIC
#define STATIC static
#endif

#define SCREEN_WIDTH  240
#define SCREEN_HEIGHT 135

static uint16_t *g_framebuffer = NULL;
static int g_key_code = -1;
static int g_key_event = 0;
static void (*g_i2s_write_callback)(const uint8_t *, size_t) = NULL;

void cardputer_host_set_i2s_write_callback(void (*callback)(const uint8_t *, size_t)) {
    g_i2s_write_callback = callback;
}

void cardputer_host_set_framebuffer(uint16_t *framebuffer) {
    g_framebuffer = framebuffer;
}

void cardputer_host_set_input(int key_code, int key_event) {
    g_key_code = key_code;
    g_key_event = key_event;
}

static bool cardputer_framebuffer_ready(void) {
    return g_framebuffer != NULL;
}

STATIC mp_obj_t cardputer_clear(mp_obj_t color_obj) {
    if (!cardputer_framebuffer_ready()) {
        return mp_const_none;
    }
    uint16_t color = (uint16_t)mp_obj_get_int(color_obj);
    for (int i = 0; i < SCREEN_WIDTH * SCREEN_HEIGHT; ++i) {
        g_framebuffer[i] = color;
    }
    return mp_const_none;
}
STATIC MP_DEFINE_CONST_FUN_OBJ_1(cardputer_clear_obj, cardputer_clear);

STATIC mp_obj_t cardputer_set_pixel(mp_obj_t x_obj, mp_obj_t y_obj, mp_obj_t color_obj) {
    if (!cardputer_framebuffer_ready()) {
        return mp_const_none;
    }
    int x = mp_obj_get_int(x_obj);
    int y = mp_obj_get_int(y_obj);
    if (x < 0 || y < 0 || x >= SCREEN_WIDTH || y >= SCREEN_HEIGHT) {
        return mp_const_none;
    }
    uint16_t color = (uint16_t)mp_obj_get_int(color_obj);
    g_framebuffer[y * SCREEN_WIDTH + x] = color;
    return mp_const_none;
}
STATIC MP_DEFINE_CONST_FUN_OBJ_3(cardputer_set_pixel_obj, cardputer_set_pixel);

STATIC mp_obj_t cardputer_fill_rect(size_t n_args, const mp_obj_t *args) {
    if (!cardputer_framebuffer_ready()) {
        return mp_const_none;
    }
    int x = mp_obj_get_int(args[0]);
    int y = mp_obj_get_int(args[1]);
    int w = mp_obj_get_int(args[2]);
    int h = mp_obj_get_int(args[3]);
    uint16_t color = (uint16_t)mp_obj_get_int(args[4]);

    if (w <= 0 || h <= 0) {
        return mp_const_none;
    }

    int x0 = x < 0 ? 0 : x;
    int y0 = y < 0 ? 0 : y;
    int x1 = x + w;
    int y1 = y + h;
    if (x1 > SCREEN_WIDTH) {
        x1 = SCREEN_WIDTH;
    }
    if (y1 > SCREEN_HEIGHT) {
        y1 = SCREEN_HEIGHT;
    }

    for (int yy = y0; yy < y1; ++yy) {
        uint16_t *row = g_framebuffer + yy * SCREEN_WIDTH;
        for (int xx = x0; xx < x1; ++xx) {
            row[xx] = color;
        }
    }
    return mp_const_none;
}
STATIC MP_DEFINE_CONST_FUN_OBJ_VAR_BETWEEN(cardputer_fill_rect_obj, 5, 5, cardputer_fill_rect);

STATIC mp_obj_t cardputer_poll_key(void) {
    if (g_key_code < 0) {
        return mp_const_none;
    }
    mp_obj_t tuple[2];
    tuple[0] = mp_obj_new_int(g_key_code);
    tuple[1] = mp_obj_new_int(g_key_event);
    g_key_code = -1;
    g_key_event = 0;
    return mp_obj_new_tuple(2, tuple);
}
STATIC MP_DEFINE_CONST_FUN_OBJ_0(cardputer_poll_key_obj, cardputer_poll_key);

STATIC mp_obj_t cardputer_screen_width(void) {
    return mp_obj_new_int(SCREEN_WIDTH);
}
STATIC MP_DEFINE_CONST_FUN_OBJ_0(cardputer_screen_width_obj, cardputer_screen_width);

STATIC mp_obj_t cardputer_screen_height(void) {
    return mp_obj_new_int(SCREEN_HEIGHT);
}
STATIC MP_DEFINE_CONST_FUN_OBJ_0(cardputer_screen_height_obj, cardputer_screen_height);

STATIC mp_obj_t cardputer_present(void) {
    return mp_const_none;
}
STATIC MP_DEFINE_CONST_FUN_OBJ_0(cardputer_present_obj, cardputer_present);

STATIC mp_obj_t cardputer_i2s_write(mp_obj_t data_obj) {
    if (g_i2s_write_callback == NULL) {
        return mp_const_none;
    }
    mp_buffer_info_t bufinfo;
    mp_get_buffer_raise(data_obj, &bufinfo, MP_BUFFER_READ);
    g_i2s_write_callback(bufinfo.buf, bufinfo.len);
    return mp_const_none;
}
STATIC MP_DEFINE_CONST_FUN_OBJ_1(cardputer_i2s_write_obj, cardputer_i2s_write);

STATIC const mp_rom_map_elem_t cardputer_module_globals_table[] = {
    { MP_ROM_QSTR(MP_QSTR___name__), MP_ROM_QSTR(MP_QSTR_cardputer) },
    { MP_ROM_QSTR(MP_QSTR_clear), MP_ROM_PTR(&cardputer_clear_obj) },
    { MP_ROM_QSTR(MP_QSTR_set_pixel), MP_ROM_PTR(&cardputer_set_pixel_obj) },
    { MP_ROM_QSTR(MP_QSTR_fill_rect), MP_ROM_PTR(&cardputer_fill_rect_obj) },
    { MP_ROM_QSTR(MP_QSTR_poll_key), MP_ROM_PTR(&cardputer_poll_key_obj) },
    { MP_ROM_QSTR(MP_QSTR_screen_width), MP_ROM_PTR(&cardputer_screen_width_obj) },
    { MP_ROM_QSTR(MP_QSTR_screen_height), MP_ROM_PTR(&cardputer_screen_height_obj) },
    { MP_ROM_QSTR(MP_QSTR_present), MP_ROM_PTR(&cardputer_present_obj) },
    { MP_ROM_QSTR(MP_QSTR_i2s_write), MP_ROM_PTR(&cardputer_i2s_write_obj) },
};

STATIC MP_DEFINE_CONST_DICT(cardputer_module_globals, cardputer_module_globals_table);

const mp_obj_module_t cardputer_module = {
    .base = { &mp_type_module },
    .globals = (mp_obj_dict_t *)&cardputer_module_globals,
};

MP_REGISTER_MODULE(MP_QSTR_cardputer, cardputer_module);
