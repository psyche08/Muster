#include <stddef.h>
#include <stdio.h>

#include "Limelight.h"

static unsigned int log_messages;

static void count_log_message(const char *format, ...) {
    (void)format;
    log_messages++;
}

int main(void) {
    const char *const rejected[] = {
        NULL, "", "0.0.0.0", "127.0.0.2", "::1", "::ffff:127.0.0.1",
        "localhost", "host.example.com", "192.0.2.10", "2001:db8::10",
        "127.0.0.1.example.com", "127.0.0.1:47989",
    };

    if (LiStartConnection(NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL, 0) != -1) {
        return 1;
    }

    SERVER_INFORMATION server = {
        .serverInfoAppVersion = "7.1.0.0",
        .serverCodecModeSupport = SCM_H264,
    };
    STREAM_CONFIGURATION configuration = {0};
    CONNECTION_LISTENER_CALLBACKS callbacks = {.logMessage = count_log_message};

    for (size_t index = 0; index < sizeof(rejected) / sizeof(rejected[0]); index++) {
        server.address = rejected[index];
        if (LiStartConnection(&server, &configuration, &callbacks, NULL, NULL, NULL, 0, NULL, 0) != -1 || log_messages != 0) {
            fprintf(stderr, "Unexpected result for endpoint case %zu\n", index);
            return 1;
        }
    }

    server.address = "127.0.0.1";
    if (LiStartConnection(&server, &configuration, &callbacks, NULL, NULL, NULL, 0, NULL, 0) != -1 || log_messages == 0) {
        fprintf(stderr, "Loopback did not reach stream configuration validation\n");
        return 1;
    }

    return 0;
}
