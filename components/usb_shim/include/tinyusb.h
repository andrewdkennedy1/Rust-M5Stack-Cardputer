#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct tusb_desc_device_t tusb_desc_device_t;

typedef struct {
    union {
        const tusb_desc_device_t *device_descriptor;
        const tusb_desc_device_t *descriptor;
    };
    const char **string_descriptor;
    int string_descriptor_count;
    bool external_phy;
    const uint8_t *configuration_descriptor;
    bool self_powered;
    int vbus_monitor_io;
} tinyusb_config_t;

esp_err_t tinyusb_driver_install(const tinyusb_config_t *config);

#ifdef __cplusplus
}
#endif
