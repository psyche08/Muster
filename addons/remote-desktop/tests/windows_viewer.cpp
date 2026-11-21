// SPDX-License-Identifier: GPL-3.0-or-later
#include "../viewer/video.h"
#include <fstream>
#include <iterator>
#include <vector>

int main(int argc, char **argv)
{
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
