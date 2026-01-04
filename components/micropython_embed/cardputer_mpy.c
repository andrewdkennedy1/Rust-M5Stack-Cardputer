#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "esp_heap_caps.h"

#include "py/compile.h"
#include "py/gc.h"
#include "py/persistentcode.h"
#include "py/runtime.h"
#include "py/stackctrl.h"
#include "py/builtin.h"
#include "py/objlist.h"
#include "port/micropython_embed.h"

#include "cardputer_mpy.h"

extern void cardputer_host_set_framebuffer(uint16_t *framebuffer);
extern void cardputer_host_set_input(int key_code, int key_event);

static uint8_t *g_heap = NULL;
static int g_heap_spiram = 0;
static mp_obj_t g_update = MP_OBJ_NULL;
static mp_obj_t g_render = MP_OBJ_NULL;
static mp_obj_t g_should_exit = MP_OBJ_NULL;
static char g_last_error[128] = {0};

static const size_t DEFAULT_PY_HEAP_BYTES = 64 * 1024;
static const size_t MIN_PY_HEAP_BYTES = 16 * 1024;
static const size_t PY_HEAP_RESERVE_INTERNAL_BYTES = 32 * 1024;
static const size_t PY_HEAP_RESERVE_SPIRAM_BYTES = 32 * 1024;

static size_t clamp_heap_size(size_t requested, size_t largest, size_t reserve) {
    if (largest == 0) {
        return 0;
    }
    size_t target = requested ? requested : DEFAULT_PY_HEAP_BYTES;
    if (target > largest) {
        target = largest;
    }
    if (largest > reserve && target > largest - reserve) {
        target = largest - reserve;
    }
    if (target < MIN_PY_HEAP_BYTES) {
        target = largest < MIN_PY_HEAP_BYTES ? largest : MIN_PY_HEAP_BYTES;
    }
    return target;
}

static uint8_t *alloc_with_fallback(
    size_t target,
    size_t min_target,
    uint32_t caps,
    size_t *out_size,
    int *out_spiram
) {
    size_t size = target;
    while (size >= min_target && size > 0) {
        uint8_t *heap = (uint8_t *)heap_caps_malloc(size, caps);
        if (heap) {
            *out_size = size;
            *out_spiram = (caps & MALLOC_CAP_SPIRAM) != 0;
            return heap;
        }
        if (size <= (8 * 1024)) {
            break;
        }
        size -= 8 * 1024;
    }
    return NULL;
}

static uint8_t *alloc_python_heap(size_t requested, size_t *out_size, int *out_spiram) {
    size_t largest_spiram = heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    size_t target = clamp_heap_size(requested, largest_spiram, PY_HEAP_RESERVE_SPIRAM_BYTES);
    if (target > 0) {
        size_t min_target = target < MIN_PY_HEAP_BYTES ? target : MIN_PY_HEAP_BYTES;
        uint8_t *heap = alloc_with_fallback(
            target,
            min_target,
            MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT,
            out_size,
            out_spiram);
        if (heap) {
            return heap;
        }
    }

    size_t largest_internal = heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
    target = clamp_heap_size(requested, largest_internal, PY_HEAP_RESERVE_INTERNAL_BYTES);
    if (target > 0) {
        size_t min_target = target < MIN_PY_HEAP_BYTES ? target : MIN_PY_HEAP_BYTES;
        uint8_t *heap = alloc_with_fallback(
            target,
            min_target,
            MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT,
            out_size,
            out_spiram);
        if (heap) {
            return heap;
        }
    }

    return NULL;
}

static void set_error(const char *message) {
    if (!message) {
        g_last_error[0] = '\0';
        return;
    }
    strncpy(g_last_error, message, sizeof(g_last_error) - 1);
    g_last_error[sizeof(g_last_error) - 1] = '\0';
}

const char *cardputer_mpy_last_error(void) {
    return g_last_error;
}

static void cardputer_mpy_cleanup(int clear_error) {
    if (g_heap) {
        mp_embed_deinit();
        heap_caps_free(g_heap);
    }
    g_heap = NULL;
    g_heap_spiram = 0;
    g_update = MP_OBJ_NULL;
    g_render = MP_OBJ_NULL;
    g_should_exit = MP_OBJ_NULL;
    if (clear_error) {
        set_error(NULL);
    }
}

static char *read_file(const char *path, size_t *out_len) {
    FILE *file = fopen(path, "rb");
    if (!file) {
        set_error("open failed");
        return NULL;
    }
    if (fseek(file, 0, SEEK_END) != 0) {
        fclose(file);
        set_error("seek failed");
        return NULL;
    }
    long size = ftell(file);
    if (size < 0) {
        fclose(file);
        set_error("tell failed");
        return NULL;
    }
    rewind(file);

    char *buffer = (char *)malloc((size_t)size + 1);
    if (!buffer) {
        fclose(file);
        set_error("alloc failed");
        return NULL;
    }

    size_t read = fread(buffer, 1, (size_t)size, file);
    fclose(file);
    if (read != (size_t)size) {
        free(buffer);
        set_error("read failed");
        return NULL;
    }
    buffer[size] = '\0';
    if (out_len) {
        *out_len = (size_t)size;
    }
    return buffer;
}

static int exec_source(const char *source, size_t len) {
    nlr_buf_t nlr;
    if (nlr_push(&nlr) == 0) {
        mp_lexer_t *lex = mp_lexer_new_from_str_len(MP_QSTR__lt_stdin_gt_, source, len, 0);
        qstr source_name = lex->source_name;
        mp_parse_tree_t parse_tree = mp_parse(lex, MP_PARSE_FILE_INPUT);
        mp_obj_t module_fun = mp_compile(&parse_tree, source_name, true);
        mp_call_function_0(module_fun);
        nlr_pop();
        return 0;
    } else {
        mp_obj_print_exception(&mp_plat_print, (mp_obj_t)nlr.ret_val);
        set_error("python exception");
        return -1;
    }
}

static int exec_source_path(const char *path) {
#if MICROPY_HAS_FILE_READER
    nlr_buf_t nlr;
    if (nlr_push(&nlr) == 0) {
        qstr source_name = qstr_from_str(path);
        mp_lexer_t *lex = mp_lexer_new_from_file(source_name);
        mp_parse_tree_t parse_tree = mp_parse(lex, MP_PARSE_FILE_INPUT);
        mp_obj_t module_fun = mp_compile(&parse_tree, source_name, true);
        mp_call_function_0(module_fun);
        nlr_pop();
        return 0;
    } else {
        mp_obj_print_exception(&mp_plat_print, (mp_obj_t)nlr.ret_val);
        set_error("python exception");
        return -1;
    }
#else
    size_t source_len = 0;
    char *source = read_file(path, &source_len);
    if (!source) {
        return -1;
    }
    int result = exec_source(source, source_len);
    free(source);
    return result;
#endif
}

static int exec_mpy(const uint8_t *mpy, size_t len) {
    nlr_buf_t nlr;
    if (nlr_push(&nlr) == 0) {
        mp_module_context_t *ctx = m_new_obj(mp_module_context_t);
        ctx->module.globals = mp_globals_get();
        mp_compiled_module_t cm = {0};
        cm.context = ctx;
        mp_raw_code_load_mem(mpy, len, &cm);
        mp_obj_t f = mp_make_function_from_proto_fun(cm.rc, ctx, MP_OBJ_NULL);
        mp_call_function_0(f);
        nlr_pop();
        return 0;
    } else {
        mp_obj_print_exception(&mp_plat_print, (mp_obj_t)nlr.ret_val);
        set_error("python exception");
        return -1;
    }
}

static int exec_mpy_path(const char *path) {
    nlr_buf_t nlr;
    if (nlr_push(&nlr) == 0) {
        mp_module_context_t *ctx = m_new_obj(mp_module_context_t);
        ctx->module.globals = mp_globals_get();
        mp_compiled_module_t cm = {0};
        cm.context = ctx;
#if MICROPY_HAS_FILE_READER
        mp_raw_code_load_file(qstr_from_str(path), &cm);
#else
        size_t source_len = 0;
        char *source = read_file(path, &source_len);
        if (!source) {
            nlr_pop();
            return -1;
        }
        mp_raw_code_load_mem((const uint8_t *)source, source_len, &cm);
        free(source);
#endif
        mp_obj_t f = mp_make_function_from_proto_fun(cm.rc, ctx, MP_OBJ_NULL);
        mp_call_function_0(f);
        nlr_pop();
        return 0;
    } else {
        mp_obj_print_exception(&mp_plat_print, (mp_obj_t)nlr.ret_val);
        set_error("python exception");
        return -1;
    }
}

static mp_obj_t get_global(const char *name, int required) {
    nlr_buf_t nlr;
    if (nlr_push(&nlr) == 0) {
        mp_obj_t key = MP_OBJ_NEW_QSTR(qstr_from_str(name));
        mp_obj_t value = mp_obj_dict_get(mp_globals_get(), key);
        nlr_pop();
        return value;
    } else {
        if (required) {
            set_error("missing function");
        }
        return MP_OBJ_NULL;
    }
}

int cardputer_mpy_start(const char *path, size_t heap_size) {
    cardputer_mpy_stop();

    size_t actual_heap_size = 0;
    g_heap = alloc_python_heap(heap_size, &actual_heap_size, &g_heap_spiram);
    if (!g_heap) {
        size_t largest_internal = heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
        size_t largest_spiram = heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
        char msg[128];
        snprintf(
            msg,
            sizeof(msg),
            "heap alloc failed (req=%u, int=%u, sp=%u)",
            (unsigned)heap_size,
            (unsigned)largest_internal,
            (unsigned)largest_spiram
        );
        set_error(msg);
        return -1;
    }
    heap_size = actual_heap_size;
    int stack_top;
    mp_stack_ctrl_init();
    mp_stack_set_limit(16 * 1024);
    mp_embed_init(g_heap, heap_size, &stack_top);
    
    #if MICROPY_PY_SYS
    // Initialize sys.path to include current directory and /sdcard/lib
    mp_obj_list_init(mp_sys_path, 0);
    mp_obj_list_append(mp_sys_path, mp_obj_new_str("", 0));
    mp_obj_list_append(mp_sys_path, mp_obj_new_str("/sdcard/lib", 11));
    #endif

    int exec_result = exec_source_path(path);
    if (exec_result != 0) {
        cardputer_mpy_cleanup(0);
        return -1;
    }

    g_update = get_global("update", 1);
    g_render = get_global("render", 0);
    g_should_exit = get_global("should_exit", 0);

    if (g_update == MP_OBJ_NULL) {
        cardputer_mpy_cleanup(0);
        return -1;
    }

    set_error(NULL);
    return 0;
}

int cardputer_mpy_start_mpy(const char *path, size_t heap_size) {
    cardputer_mpy_stop();

    size_t actual_heap_size = 0;
    g_heap = alloc_python_heap(heap_size, &actual_heap_size, &g_heap_spiram);
    if (!g_heap) {
        size_t largest_internal = heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
        size_t largest_spiram = heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
        char msg[128];
        snprintf(
            msg,
            sizeof(msg),
            "heap alloc failed (req=%u, int=%u, sp=%u)",
            (unsigned)heap_size,
            (unsigned)largest_internal,
            (unsigned)largest_spiram
        );
        set_error(msg);
        return -1;
    }
    heap_size = actual_heap_size;
    int stack_top;
    mp_stack_ctrl_init();
    mp_stack_set_limit(16 * 1024);
    mp_embed_init(g_heap, heap_size, &stack_top);
    
    #if MICROPY_PY_SYS
    // Initialize sys.path to include current directory and /sdcard/lib
    mp_obj_list_init(mp_sys_path, 0);
    mp_obj_list_append(mp_sys_path, mp_obj_new_str("", 0));
    mp_obj_list_append(mp_sys_path, mp_obj_new_str("/sdcard/lib", 11));
    #endif

    int exec_result = exec_mpy_path(path);
    if (exec_result != 0) {
        cardputer_mpy_cleanup(0);
        return -1;
    }

    g_update = get_global("update", 1);
    g_render = get_global("render", 0);
    g_should_exit = get_global("should_exit", 0);

    if (g_update == MP_OBJ_NULL) {
        cardputer_mpy_cleanup(0);
        return -1;
    }

    set_error(NULL);
    return 0;
}

static int call_func(mp_obj_t func, size_t n_args, mp_obj_t *args, mp_obj_t *out) {
    nlr_buf_t nlr;
    if (nlr_push(&nlr) == 0) {
        mp_obj_t result = mp_call_function_n_kw(func, n_args, 0, args);
        nlr_pop();
        if (out) {
            *out = result;
        }
        return 0;
    } else {
        mp_obj_print_exception(&mp_plat_print, (mp_obj_t)nlr.ret_val);
        set_error("python exception");
        return -1;
    }
}

int cardputer_mpy_tick(uint32_t dt_ms, int32_t key_code, int32_t key_event, uint16_t *framebuffer) {
    if (g_update == MP_OBJ_NULL) {
        set_error("runtime not initialized");
        return -1;
    }

    cardputer_host_set_framebuffer(framebuffer);
    cardputer_host_set_input(key_code, key_event);

    mp_obj_t update_args[3];
    update_args[0] = mp_obj_new_int_from_uint(dt_ms);
    update_args[1] = mp_obj_new_int(key_code);
    update_args[2] = mp_obj_new_int(key_event);
    if (call_func(g_update, 3, update_args, NULL) != 0) {
        return -1;
    }

    if (g_render != MP_OBJ_NULL) {
        if (call_func(g_render, 0, NULL, NULL) != 0) {
            return -1;
        }
    }

    if (g_should_exit != MP_OBJ_NULL) {
        mp_obj_t result = MP_OBJ_NULL;
        if (call_func(g_should_exit, 0, NULL, &result) != 0) {
            return -1;
        }
        if (mp_obj_is_true(result)) {
            return 1;
        }
    }

    return 0;
}

void cardputer_mpy_stop(void) {
    cardputer_mpy_cleanup(1);
}
