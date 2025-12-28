file(GLOB_RECURSE MICROPY_SRC
    "${CMAKE_CURRENT_LIST_DIR}/embedded/py/*.c"
    "${CMAKE_CURRENT_LIST_DIR}/embedded/extmod/*.c"
    "${CMAKE_CURRENT_LIST_DIR}/embedded/shared/runtime/*.c"
    "${CMAKE_CURRENT_LIST_DIR}/embedded/port/*.c"
)

idf_component_register(
    SRCS
        ${MICROPY_SRC}
        "cardputer_module.c"
        "cardputer_mpy.c"
    INCLUDE_DIRS
        "."
        "embedded"
        "embedded/py"
        "embedded/extmod"
        "embedded/shared/runtime"
        "embedded/genhdr"
        "embedded/port"
)
