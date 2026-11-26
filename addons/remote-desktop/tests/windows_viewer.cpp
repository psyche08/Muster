// SPDX-License-Identifier: GPL-3.0-or-later
#include "../viewer/video.h"
#include "../viewer/keyboard.h"
#include <string>
#include <fstream>
#include <iterator>
#include <vector>

bool keyboard_input()
{
    viewer_input::Keyboard keyboard;
    std::string text;
    std::vector<short> keys;
    auto send = [&](UINT msg, WPARAM value, bool active, bool shortcut = false) {
        return keyboard.handle(msg, value, active, shortcut,
            [&](const char *s, int n) { text.append(s, n); },
            [&](short key) { keys.push_back(key); });
    };
    unsigned char record[8] = {'M','V','W','1',1,1,0xc9,0x01};
    viewer_input::Bootstrap access;
    if (!viewer_input::decode(record, access) || access.keyboard || access.rtsp != 51478) return false;
    record[4] = 2;
    if (viewer_input::decode(record, access)) return false;
    record[3] = '2';
    for (int mode = 0; mode < 3; ++mode) {
        record[4] = static_cast<unsigned char>(mode);
        if (!viewer_input::decode(record, access) || access.rtsp != 51457 || access.keyboard != (mode == 2)) return false;
        send(WM_CHAR, 'R', access.keyboard);
    }
    send(WM_CHAR, 0xd83d, true);
    send(WM_KILLFOCUS, 0, true);
    send(WM_CHAR, 0xde00, true);
    send(WM_CHAR, 'x', false);
    send(WM_CHAR, 0xd83d, true);
    send(WM_CHAR, 0xde00, true);
    send(WM_KEYDOWN, VK_BACK, true);
    send(WM_CHAR, 8, true);
    send(WM_KEYDOWN, VK_RETURN, true);
    send(WM_CHAR, 13, true);
    send(WM_KEYDOWN, VK_RETURN, true, true);
    send(WM_KEYDOWN, VK_BACK, false);
    return text == "R\xf0\x9f\x98\x80" && keys == std::vector<short>{VK_BACK, VK_RETURN};
}

// A private, unshown standard edit control is a local input sink. Read its
// actual buffer after delivery. This test never focuses another application.
bool edit_round_trip()
{
    HWND edit = CreateWindowExW(0, L"EDIT", L"", WS_POPUP | ES_MULTILINE | ES_WANTRETURN,
                                0, 0, 200, 80, nullptr, nullptr, GetModuleHandleW(nullptr), nullptr);
    if (!edit) return false;
    viewer_input::Keyboard keyboard;
    auto deliver = [&](UINT msg, WPARAM value, bool enabled) {
        keyboard.handle(msg, value, enabled, false, [&](const char *text, int size) {
            wchar_t wide[3]{};
            int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, size, wide, 2);
            if (n > 0) SendMessageW(edit, EM_REPLACESEL, TRUE, reinterpret_cast<LPARAM>(wide));
        }, [&](short key) { SendMessageW(edit, WM_CHAR, key, 0); });
    };
    deliver(WM_CHAR, 'R', true);
    deliver(WM_CHAR, 'x', true);
    deliver(WM_KEYDOWN, VK_BACK, true);
    deliver(WM_CHAR, '7', true);
    deliver(WM_CHAR, 'M', true);
    deliver(WM_KEYDOWN, VK_RETURN, true);
    deliver(WM_CHAR, 0x6d4b, true);
    deliver(WM_CHAR, 'x', false);
    wchar_t actual[32]{};
    GetWindowTextW(edit, actual, 32);
    DestroyWindow(edit);
    return std::wstring(actual) == L"R7M\r\n\u6d4b";
}

int main(int argc, char **argv)
{
    if (!keyboard_input() || !edit_round_trip()) return 6;
    if (argc != 2 || video::setup(VIDEO_FORMAT_H264, 1280, 720, 30, nullptr, 0) != 0)
        return 1;
    std::ifstream in(argv[1], std::ios::binary);
    std::vector<char> data{std::istreambuf_iterator<char>(in), {}};
    if (data.empty())
        return 1;
    LENTRY part{};
    part.data = data.data();
    part.length = int(data.size());
    DECODE_UNIT unit{};
    unit.bufferList = &part;
    unit.fullLength = part.length;
    if (video::decode(&unit) != DR_OK)
        return 2;
    HDC dc = CreateCompatibleDC(nullptr);
    BITMAPINFO info{};
    info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    info.bmiHeader.biWidth = 640;
    info.bmiHeader.biHeight = -480;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    void *pixels = nullptr;
    auto bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &pixels, nullptr, 0);
    if (!bitmap)
        return 3;
    auto old = SelectObject(dc, bitmap);
    RECT client{0, 0, 640, 480};
    bool painted = video::draw(dc, client);
    auto center = GetPixel(dc, 320, 240);
    auto border = GetPixel(dc, 320, 10);
    SelectObject(dc, old);
    DeleteObject(bitmap);
    DeleteDC(dc);
    video::cleanup();
    if (!painted || GetBValue(center) < 240 || GetRValue(center) > 10 || GetGValue(center) > 10 ||
        border != RGB(0, 0, 0))
        return 4;
    auto area = video::viewport(client);
    short x, y;
    if (area.top != 60 || area.bottom != 420 || !video::position(area, {639, 419}, x, y) || x != 1279 || y != 719)
        return 5;
    return 0;
}
