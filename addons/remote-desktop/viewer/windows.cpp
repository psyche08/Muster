// SPDX-License-Identifier: GPL-3.0-or-later
// Private native media child. Shared Rust owns the peer and engine lifetime.
#include "video.h"
#include <algorithm>
#include <array>
#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>
#include <vector>
#include <windowsx.h>

namespace
{
using video::height;
using video::width;
constexpr UINT stopMessage = WM_APP + 1, readyMessage = WM_APP + 2;
HWND window, parent;
std::atomic<bool> stopping;
bool canControl, receivedFrame, connected;
std::mutex statusLock;

void reportFailure(int stage, int code)
{
    // Only the local, fixed stage name and numeric library error cross this pipe.
    const char* name = stage > STAGE_NONE && stage < STAGE_MAX ? LiGetStageName(stage) : "connection";
    std::lock_guard lock(statusLock);
    std::fprintf(stdout, "ERROR %s (%d)\n", name, code);
    std::fflush(stdout);
}
RECT viewport(HWND hwnd)
{
    RECT client{};
    GetClientRect(hwnd, &client);
    return video::viewport(client);
}
void paint(HWND hwnd)
{
    PAINTSTRUCT ps{};
    HDC dc = BeginPaint(hwnd, &ps);
    FillRect(dc, &ps.rcPaint, static_cast<HBRUSH>(GetStockObject(BLACK_BRUSH)));
    RECT client{};
    GetClientRect(hwnd, &client);
    if (video::draw(dc, client) && !receivedFrame)
    {
        receivedFrame = true;
        std::lock_guard lock(statusLock);
        std::fputs("FRAME\n", stdout);
        std::fflush(stdout);
    }
    EndPaint(hwnd, &ps);
}
void pointer(HWND hwnd, LPARAM pos)
{
    short x, y;
    if (video::position(viewport(hwnd), {GET_X_LPARAM(pos), GET_Y_LPARAM(pos)}, x, y))
        LiSendMousePositionEvent(x, y, video::width, video::height);
}

LRESULT CALLBACK message(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    static unsigned pressed;
    switch (msg)
    {
    case WM_PAINT:
        paint(hwnd);
        return 0;
    case WM_ERASEBKGND:
        return 1;
    case readyMessage:
        connected = true;
        return 0;
    case stopMessage:
    case WM_CLOSE:
        DestroyWindow(hwnd);
        return 0;
    case WM_DESTROY:
        stopping = true;
        LiInterruptConnection();
        PostQuitMessage(0);
        return 0;
    case WM_TIMER: {
        RECT r{}, own{};
        if (!GetClientRect(parent, &r))
        {
            PostMessage(hwnd, WM_CLOSE, 0, 0);
            return 0;
        }
        GetClientRect(hwnd, &own);
        if (own.right != r.right || own.bottom != r.bottom)
            SetWindowPos(hwnd, nullptr, 0, 0, r.right, r.bottom, SWP_NOZORDER | SWP_NOACTIVATE);
        return 0;
    }
    }
    if (canControl && connected && receivedFrame)
    {
        switch (msg)
        {
        case WM_MOUSEMOVE: {
            auto area = viewport(hwnd);
            if (pressed || PtInRect(&area, {GET_X_LPARAM(lp), GET_Y_LPARAM(lp)}))
                pointer(hwnd, lp);
            return 0;
        }
        case WM_LBUTTONDOWN:
        case WM_RBUTTONDOWN:
        case WM_MBUTTONDOWN:
        case WM_LBUTTONUP:
        case WM_RBUTTONUP:
        case WM_MBUTTONUP: {
            bool down = msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN;
            int button = msg == WM_LBUTTONDOWN || msg == WM_LBUTTONUP   ? BUTTON_LEFT
                         : msg == WM_RBUTTONDOWN || msg == WM_RBUTTONUP ? BUTTON_RIGHT
                                                                        : BUTTON_MIDDLE;
            POINT p{GET_X_LPARAM(lp), GET_Y_LPARAM(lp)};
            auto area = viewport(hwnd);
            if (down && !PtInRect(&area, p))
                return 0;
            if (down)
            {
                SetCapture(hwnd);
                pressed |= 1u << button;
            }
            else if ((pressed & (1u << button)) == 0)
                return 0;
            else
                pressed &= ~(1u << button);
            pointer(hwnd, lp);
            LiSendMouseButtonEvent(down ? BUTTON_ACTION_PRESS : BUTTON_ACTION_RELEASE, button);
            if (!pressed && GetCapture() == hwnd)
                ReleaseCapture();
            return 0;
        }
        case WM_CAPTURECHANGED:
            for (int b : {BUTTON_LEFT, BUTTON_RIGHT, BUTTON_MIDDLE})
                if (pressed & (1u << b))
                    LiSendMouseButtonEvent(BUTTON_ACTION_RELEASE, b);
            pressed = 0;
            return 0;
        case WM_MOUSEWHEEL: {
            POINT mouse{GET_X_LPARAM(lp), GET_Y_LPARAM(lp)};
            auto area = viewport(hwnd);
            if (ScreenToClient(hwnd, &mouse) && PtInRect(&area, mouse))
                LiSendHighResScrollEvent(GET_WHEEL_DELTA_WPARAM(wp));
            return 0;
        }
        }
    }
    return DefWindowProc(hwnd, msg, wp, lp);
}
} // namespace

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE, PWSTR, int)
{
    HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
    if (GetFileType(input) != FILE_TYPE_PIPE)
        return 1;
    std::array<unsigned char, 36> init{};
    // MVW1, mouse and encrypted RTSP flags, port (BE16), parent HWND (BE64), RI key, IV prefix.
    DWORD received = 0;
    while (received < init.size())
    {
        DWORD count = 0;
        if (!ReadFile(input, init.data() + received, static_cast<DWORD>(init.size()) - received, &count, nullptr) || count == 0)
            return 1;
        received += count;
    }
    if (std::memcmp(init.data(), "MVW1", 4) ||
        init[4] > 1 || init[5] > 1)
        return 1;
    canControl = init[4] != 0;
    unsigned port = unsigned(init[6]) * 256 + init[7];
    if (port < 49157 || port > 65514)
        return 1;
    uintptr_t handle = 0;
    for (size_t i = 8; i < 16; ++i)
        handle = (handle << 8) | init[i];
    parent = reinterpret_cast<HWND>(handle);
    if (!IsWindow(parent))
        return 1;
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    WNDCLASS wc{};
    wc.lpfnWndProc = message;
    wc.hInstance = instance;
    wc.lpszClassName = L"MusterMoonlight";
    wc.hCursor = LoadCursor(nullptr, IDC_ARROW);
    if (!RegisterClass(&wc))
        return 1;
    RECT r{};
    GetClientRect(parent, &r);
    window = CreateWindowExW(0, wc.lpszClassName, L"Paired desktop", WS_CHILD | WS_VISIBLE, 0, 0, r.right, r.bottom,
                             parent, nullptr, instance, nullptr);
    if (!window)
        return 1;
    SetTimer(window, 1, 150, nullptr);
    std::thread([input] {
        // A blocked CRT stream read holds stdin's lock during process exit.
        char stop;
        DWORD count;
        ReadFile(input, &stop, 1, &count, nullptr);
        PostMessage(window, stopMessage, 0, 0);
    }).detach();
    int result = -1;
    std::thread stream([&] {
        STREAM_CONFIGURATION config{};
        LiInitializeStreamConfiguration(&config);
        config.width = width;
        config.height = height;
        config.fps = 30;
        config.bitrate = 3500;
        config.packetSize = 960;
        config.streamingRemotely = STREAM_CFG_REMOTE;
        config.audioConfiguration = AUDIO_CONFIGURATION_STEREO;
        config.supportedVideoFormats = VIDEO_FORMAT_H264;
        config.encryptionFlags = ENCFLG_ALL;
        std::memcpy(config.remoteInputAesKey, init.data() + 16, 16);
        std::memcpy(config.remoteInputAesIv, init.data() + 32, 4);
        std::string url = std::string(init[5] ? "rtspenc://" : "rtsp://") + "127.0.0.1:" + std::to_string(port + 21);
        SERVER_INFORMATION server{};
        LiInitializeServerInformation(&server);
        server.address = "127.0.0.1";
        server.serverInfoAppVersion = "7.1.431.-1";
        server.serverInfoGfeVersion = "3.23.0.74";
        server.serverCodecModeSupport = SCM_H264;
        server.rtspSessionUrl = url.c_str();
        DECODER_RENDERER_CALLBACKS video{};
        LiInitializeVideoCallbacks(&video);
        video.setup = ::video::setup;
        video.cleanup = ::video::cleanup;
        video.submitDecodeUnit = [](PDECODE_UNIT unit) {
            int status = ::video::decode(unit);
            if (status == DR_OK)
                InvalidateRect(window, nullptr, FALSE);
            return status;
        };
        video.capabilities = CAPABILITY_DIRECT_SUBMIT;
        CONNECTION_LISTENER_CALLBACKS callbacks{};
        LiInitializeConnectionCallbacks(&callbacks);
        callbacks.stageFailed = reportFailure;
        callbacks.connectionTerminated = [](int code) {
            reportFailure(STAGE_NONE, code);
            PostMessage(window, stopMessage, 0, 0);
        };
        callbacks.connectionStarted = [] { PostMessage(window, readyMessage, 0, 0); };
        if (!stopping)
            result = LiStartConnection(&server, &config, &callbacks, &video, nullptr, nullptr, 0, nullptr, 0);
        if (result != 0)
            PostMessage(window, stopMessage, 0, 0);
    });
    MSG m{};
    while (GetMessage(&m, nullptr, 0, 0) > 0)
    {
        TranslateMessage(&m);
        DispatchMessage(&m);
    }
    stream.join();
    if (result == 0)
        LiStopConnection();
    SecureZeroMemory(init.data(), init.size());
    return result == 0 ? 0 : 1;
}
