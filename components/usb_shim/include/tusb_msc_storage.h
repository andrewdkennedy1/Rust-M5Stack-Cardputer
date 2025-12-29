#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif


typedef struct {
    bool is_mounted;
} tinyusb_msc_event_mount_changed_data_t;

typedef enum {
    TINYUSB_MSC_EVENT_MOUNT_CHANGED
} tinyusb_msc_event_type_t;

typedef struct {
    tinyusb_msc_event_type_t type;
    union {
        tinyusb_msc_event_mount_changed_data_t mount_changed_data;
    };
} tinyusb_msc_event_t;

typedef void (*tusb_msc_callback_t)(tinyusb_msc_event_t *event);

typedef struct {
    void *card;
    tusb_msc_callback_t callback_mount_changed;
} tinyusb_msc_sdmmc_config_t;

esp_err_t tinyusb_msc_storage_init_sdmmc(const tinyusb_msc_sdmmc_config_t *config);
esp_err_t tinyusb_msc_storage_mount(const char *base_path);
esp_err_t tinyusb_msc_storage_unmount(void);
void tinyusb_msc_storage_deinit(void);
bool tinyusb_msc_storage_in_use_by_usb_host(void);

#ifdef __cplusplus
}
#endif
