// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <windows.h>
extern "C"
{
#include "Limelight.h"
}
namespace video
{
constexpr int width = 1280, height = 720;
RECT viewport(RECT client);
bool position(RECT area, POINT mouse, short &x, short &y);
int setup(int format, int w, int h, int rate, void *context, int flags);
void cleanup();
int decode(PDECODE_UNIT unit);
bool draw(HDC dc, RECT client);
} // namespace video
