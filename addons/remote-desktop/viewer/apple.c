// SPDX-License-Identifier: GPL-3.0-or-later
// The same adapter runs inside the iOS viewer or as a private macOS child.
#include <Limelight.h>
#include <arpa/inet.h>
#include <errno.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>

static pthread_mutex_t session_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t output_lock = PTHREAD_MUTEX_INITIALIZER;
static int video_fd;
static atomic_bool started, stopping;
struct input { int fd, width, height, mouse; };

static int transfer(int fd, void *data, size_t length, bool sending) {
    char *p = data;
    while (length) {
        ssize_t n = sending ? write(fd, p, length) : read(fd, p, length);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) return -1;
        p += n;
        length -= n;
    }
    return 0;
}

static int frame(PDECODE_UNIT unit) {
    if (unit->fullLength <= 0 || unit->fullLength > 4*1024*1024) return DR_NEED_IDR;
    int remaining = unit->fullLength;
    for (PLENTRY p = unit->bufferList; p; p = p->next) {
        if (p->length <= 0 || p->length > remaining) return DR_NEED_IDR;
        remaining -= p->length;
    }
    if (remaining) return DR_NEED_IDR;
    uint32_t length = htonl(unit->fullLength);
    pthread_mutex_lock(&output_lock);
    int err = transfer(video_fd, &length, 4, true);
    for (PLENTRY p = unit->bufferList; p && !err; p = p->next)
        err = transfer(video_fd, p->data, p->length, true);
    pthread_mutex_unlock(&output_lock);
    return err ? DR_NEED_IDR : DR_OK;
}

static void ended(int code) {
    (void)code;
    uint32_t end = 0;
    pthread_mutex_lock(&output_lock);
    transfer(video_fd, &end, sizeof(end), true);
    pthread_mutex_unlock(&output_lock);
}
static void connected(void) { atomic_store(&started, true); }

static uint16_t u16(const unsigned char *p) { return (p[0]<<8) | p[1]; }
static void *input_loop(void *ctx) {
    struct input *in = ctx;
    unsigned char p[6];
    bool held = false;
    while (transfer(in->fd, p, sizeof(p), false) == 0) {
        if (!in->mouse || !atomic_load(&started)) continue;
        if (p[0] > 6 || held != (p[0] == 5 || p[0] == 6)) break;
        LiSendMousePositionEvent((uint32_t)u16(p+1)*(in->width-1)/65535,
                                 (uint32_t)u16(p+3)*(in->height-1)/65535,
                                 in->width, in->height);
        switch (p[0]) {
        case 1: case 2: {
            int button = p[0] == 1 ? BUTTON_LEFT : BUTTON_RIGHT;
            LiSendMouseButtonEvent(BUTTON_ACTION_PRESS, button);
            LiSendMouseButtonEvent(BUTTON_ACTION_RELEASE, button);
            break;
        }
        case 3: LiSendScrollEvent((signed char)p[5]); break;
        case 4: case 6:
            held = p[0] == 4;
            LiSendMouseButtonEvent(held ? BUTTON_ACTION_PRESS : BUTTON_ACTION_RELEASE, BUTTON_LEFT);
            break;
        }
    }
    if (held) LiSendMouseButtonEvent(BUTTON_ACTION_RELEASE, BUTTON_LEFT);
    atomic_store(&stopping, true);
    LiInterruptConnection();
    return NULL;
}

// MAV1 | mouse | width,height,fps (BE16) | bitrate (BE32) | RI key and IV prefix.
int muster_moonlight_run(int input_fd, int output_fd) {
#ifndef MUSTER_MOONLIGHT_CHILD
    int no_sigpipe = 1;
    if (setsockopt(output_fd, SOL_SOCKET, SO_NOSIGPIPE, &no_sigpipe, sizeof(no_sigpipe)) != 0) return 1;
#endif
    unsigned char init[35];
    if (transfer(input_fd, init, sizeof(init), false) || memcmp(init, "MAV1", 4) || init[4]>1) return 1;
    struct input input = {input_fd, u16(init+5), u16(init+7), init[4]};
    STREAM_CONFIGURATION config;
    LiInitializeStreamConfiguration(&config);
    config.width=input.width; config.height=input.height; config.fps=u16(init+9);
    uint32_t bitrate;
    memcpy(&bitrate,init+11,sizeof(bitrate));
    config.bitrate=ntohl(bitrate);
    if (config.width<2 || config.height<2 || config.width>2560 || config.height>2560 ||
        config.fps<1 || config.fps>60 || config.bitrate<1 || config.bitrate>100000) return 1;
    if (pthread_mutex_trylock(&session_lock)) return 1;
    video_fd=output_fd;
    atomic_store(&started,false); atomic_store(&stopping,false);
    config.packetSize=960;
    config.streamingRemotely=STREAM_CFG_REMOTE;
    config.audioConfiguration=AUDIO_CONFIGURATION_STEREO;
    config.supportedVideoFormats=VIDEO_FORMAT_H264;
    config.encryptionFlags=ENCFLG_ALL;
    memcpy(config.remoteInputAesKey,init+15,16);
    memcpy(config.remoteInputAesIv,init+31,4);
    SERVER_INFORMATION server;
    LiInitializeServerInformation(&server);
    server.address="127.0.0.1";
    server.serverInfoAppVersion="7.1.431.-1";
    server.serverInfoGfeVersion="3.23.0.74";
    server.serverCodecModeSupport=SCM_H264;
    server.rtspSessionUrl="rtspenc://127.0.0.1:51410";
    DECODER_RENDERER_CALLBACKS video;
    LiInitializeVideoCallbacks(&video);
    video.submitDecodeUnit=frame;
    video.capabilities=CAPABILITY_DIRECT_SUBMIT;
    CONNECTION_LISTENER_CALLBACKS callbacks;
    LiInitializeConnectionCallbacks(&callbacks);
    callbacks.connectionTerminated=ended;
    callbacks.connectionStarted=connected;
    pthread_t reader;
    int result=pthread_create(&reader,NULL,input_loop,&input);
    if (result==0) {
        result=atomic_load(&stopping) ? -1 : LiStartConnection(&server,&config,&callbacks,&video,NULL,NULL,0,NULL,0);
        if (result) ended(result);
        pthread_join(reader,NULL);
        if (!result) LiStopConnection();
    }
    pthread_mutex_unlock(&session_lock);
    return result;
}
#ifdef MUSTER_MOONLIGHT_CHILD
int main(void) { return muster_moonlight_run(STDIN_FILENO,STDOUT_FILENO); }
#endif

#ifndef MUSTER_MOONLIGHT_CHILD
#include <sys/socket.h>
// iOS runs the broker and engine in this process. Check live kernel descriptors
// before admitting a loopback endpoint, including on cache hits after fd reuse.
static bool matches_socket(int fd, uint16_t port, bool tcp) {
    struct sockaddr_in addr = {0};
    socklen_t len = sizeof(addr);
    int type = 0;
    socklen_t size = sizeof(type);
    return getsockname(fd, (struct sockaddr *)&addr, &len) == 0 &&
        addr.sin_family == AF_INET && ntohs(addr.sin_port) == port &&
        (addr.sin_addr.s_addr == htonl(INADDR_LOOPBACK) || addr.sin_addr.s_addr == INADDR_ANY) &&
        getsockopt(fd, SOL_SOCKET, SO_TYPE, &type, &size) == 0 &&
        type == (tcp ? SOCK_STREAM : SOCK_DGRAM);
}
bool muster_moonlight_owns(uint16_t port, bool tcp) {
    static _Atomic int recent[16];
    unsigned slot = port % 16;
    if (matches_socket(atomic_load(&recent[slot]), port, tcp)) return true;
    int limit = getdtablesize();
    for (int fd=0; fd<limit; fd++) {
        if (matches_socket(fd,port,tcp)) {
            atomic_store(&recent[slot],fd);
            return true;
        }
    }
    return false;
}
#endif
