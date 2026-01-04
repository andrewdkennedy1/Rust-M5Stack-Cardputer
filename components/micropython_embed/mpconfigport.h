// MicroPython embed configuration for Cardputer.
#include <port/mpconfigport_common.h>

#define MICROPY_CONFIG_ROM_LEVEL                (MICROPY_CONFIG_ROM_LEVEL_MINIMUM)
#define MICROPY_ENABLE_COMPILER                 (1)
#define MICROPY_ENABLE_GC                       (1)
#define MICROPY_PY_GC                           (1)
#define MICROPY_GCREGS_SETJMP                   (1)
#define MICROPY_PERSISTENT_CODE_LOAD            (1)
#define MICROPY_PY_SYS                          (1)
#define MICROPY_PY_IO                           (1)
#define MICROPY_PY_STRUCT                       (1)
#define MICROPY_PY_UOS                          (1)
#define MICROPY_PY_BUILTINS_HELP                (1)
#define MICROPY_READER_POSIX                    (1)
#define MICROPY_ERROR_REPORTING                 (MICROPY_ERROR_REPORTING_NORMAL)
