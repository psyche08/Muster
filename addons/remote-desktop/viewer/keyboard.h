// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <windows.h>
#include <cstdint>
#include <cstring>

namespace viewer_input
{
// MVW1 carries a base port and has no keyboard mode. MVW2 carries the
// actual local RTSP port and the receiver's negotiated input mode.
struct Bootstrap {
    bool mouse{};
    bool keyboard{};
    unsigned rtsp{};
};
inline bool decode(const unsigned char *record, Bootstrap &out)
{
    const bool legacy = std::memcmp(record, "MVW1", 4) == 0;
    if ((!legacy && std::memcmp(record, "MVW2", 4) != 0)
        || record[4] > (legacy ? 1 : 2) || record[5] > 1)
        return false;
    unsigned port = (unsigned(record[6]) << 8) | record[7];
    if (!port || (legacy && (port < 49157 || port > 65514)))
        return false;
    out = {record[4] != 0, !legacy && record[4] == 2, legacy ? port + 21 : port};
    return true;
}

class Keyboard {
    wchar_t high{};
public:
    void reset() { high = 0; }

    // WM_CHAR is committed text in the window's input locale. Keep a high
    // surrogate only until its matching unit or a change of focus.
    template<class Text, class Key>
    bool handle(UINT msg, WPARAM value, bool active, bool shortcut, Text text, Key key)
    {
        if (!active || msg == WM_KILLFOCUS) {
            reset();
            return false;
        }
        if (msg == WM_KEYDOWN && !shortcut && (value == VK_RETURN || value == VK_BACK)) {
            reset();
            key(static_cast<short>(value));
            return true;
        }
        if (msg != WM_CHAR)
            return false;
        wchar_t unit = static_cast<wchar_t>(value);
        wchar_t units[2]{};
        int count = 1;
        if (unit >= 0xd800 && unit <= 0xdbff) {
            high = unit;
            return true;
        }
        if (unit >= 0xdc00 && unit <= 0xdfff) {
            if (!high) return true;
            units[0] = high;
            units[1] = unit;
            count = 2;
        } else {
            units[0] = unit;
        }
        reset();
        if (unit < 0x20 || (unit >= 0x7f && unit <= 0x9f))
            return true;
        char bytes[4];
        int n = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, units, count, bytes, sizeof bytes, nullptr, nullptr);
        if (n > 0) text(bytes, n);
        return true;
    }
};
}
