// SPDX-License-Identifier: GPL-3.0-or-later
#include "video.h"
#include <algorithm>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <vector>
extern "C"
{
#include <libavcodec/avcodec.h>
#include <libswscale/swscale.h>
}
namespace video
{
namespace
{
AVCodecContext *decoder;
AVFrame *frame;
SwsContext *scaler;
std::mutex imageLock;
std::vector<uint8_t> image;

} // namespace
RECT viewport(RECT client)
{
    int w = client.right, h = client.bottom;
    if (w <= 0 || h <= 0)
        return {};
    int vw = std::min(w, int(int64_t(h) * width / height));
    int vh = std::min(h, int(int64_t(w) * height / width));
    return {(w - vw) / 2, (h - vh) / 2, (w + vw) / 2, (h + vh) / 2};
}

void cleanup()
{
    sws_freeContext(scaler);
    scaler = nullptr;
    av_frame_free(&frame);
    avcodec_free_context(&decoder);
}

int setup(int format, int w, int h, int, void *, int)
{
    if (format != VIDEO_FORMAT_H264 || w != width || h != height)
        return -1;
    auto codec = avcodec_find_decoder(AV_CODEC_ID_H264);
    if (!codec)
        return -1;
    decoder = avcodec_alloc_context3(codec);
    frame = av_frame_alloc();
    if (!decoder || !frame)
    {
        cleanup();
        return -1;
    }
    decoder->max_pixels = width * height;
    decoder->thread_count = 2;
    decoder->thread_type = FF_THREAD_SLICE;
    int result = avcodec_open2(decoder, codec, nullptr);
    if (result < 0)
        cleanup();
    return result;
}

int decode(PDECODE_UNIT unit)
{
    if (!decoder || unit->fullLength <= 0 || unit->fullLength > 4 * 1024 * 1024)
        return DR_NEED_IDR;
    AVPacket *packet = av_packet_alloc();
    if (!packet || av_new_packet(packet, unit->fullLength) < 0)
    {
        av_packet_free(&packet);
        return DR_NEED_IDR;
    }
    int copied = 0;
    for (auto *part = unit->bufferList; part; part = part->next)
    {
        if (part->length <= 0 || part->length > unit->fullLength - copied)
        {
            av_packet_free(&packet);
            return DR_NEED_IDR;
        }
        std::memcpy(packet->data + copied, part->data, part->length);
        copied += part->length;
    }
    int result = copied == unit->fullLength ? avcodec_send_packet(decoder, packet) : -1;
    av_packet_free(&packet);
    if (result < 0)
        return DR_NEED_IDR;
    while ((result = avcodec_receive_frame(decoder, frame)) == 0)
    {
        if (frame->width != width || frame->height != height)
            return DR_NEED_IDR;
        scaler = sws_getCachedContext(scaler, width, height, AVPixelFormat(frame->format), width, height,
                                      AV_PIX_FMT_BGRA, SWS_BILINEAR, nullptr, nullptr, nullptr);
        if (!scaler)
            return DR_NEED_IDR;
        {
            std::lock_guard lock(imageLock);
            image.resize(size_t(width) * height * 4);
            uint8_t *dst[] = {image.data(), nullptr, nullptr, nullptr};
            int strides[] = {width * 4, 0, 0, 0};
            if (sws_scale(scaler, frame->data, frame->linesize, 0, height, dst, strides) != height)
                return DR_NEED_IDR;
        }
    }
    return result == AVERROR(EAGAIN) || result == AVERROR_EOF ? DR_OK : DR_NEED_IDR;
}

bool position(RECT area, POINT mouse, short &x, short &y)
{
    int w = area.right - area.left, h = area.bottom - area.top;
    if (w <= 0 || h <= 0)
        return false;
    x = short(int64_t(std::clamp(int(mouse.x - area.left), 0, w - 1)) * (width - 1) / std::max(w - 1, 1));
    y = short(int64_t(std::clamp(int(mouse.y - area.top), 0, h - 1)) * (height - 1) / std::max(h - 1, 1));
    return true;
}

bool draw(HDC dc, RECT client)
{
    {
        std::lock_guard lock(imageLock);
        if (!image.empty())
        {
            BITMAPINFO info{};
            info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
            info.bmiHeader.biWidth = width;
            info.bmiHeader.biHeight = -height;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            auto r = viewport(client);
            int drawn = StretchDIBits(dc, r.left, r.top, r.right - r.left, r.bottom - r.top, 0, 0, width, height,
                                      image.data(), &info, DIB_RGB_COLORS, SRCCOPY);
            return drawn > 0;
        }
    }
    return false;
}
} // namespace video
