/**
 * @file src/platform/macos/display.mm
 * @brief Definitions for display capture on macOS.
 */

// standard includes
#include <charconv>
#include <chrono>
#include <optional>
#include <string_view>
#include <thread>

// local includes
#include "src/config.h"
#include "src/muster_session.h"
#include "src/display_device.h"
#include "src/logging.h"
#include "src/platform/common.h"
#include "src/platform/macos/av_img_t.h"
#include "src/platform/macos/screen_video.h"
#include "src/platform/macos/misc.h"
#include "src/platform/macos/nv12_zero_device.h"

// Avoid conflict between AVFoundation and libavutil both defining AVMediaType
/**
 * @def AVMediaType
 * @brief Macro for AV media type.
 */
#define AVMediaType AVMediaType_FFmpeg
#include "src/video.h"
#undef AVMediaType

namespace platf {
  using namespace std::literals;

  namespace {
    std::optional<CGDirectDisplayID> parse_display_id(std::string_view display_name) {
      if (display_name.empty()) {
        return std::nullopt;
      }

      CGDirectDisplayID display_id {};
      const auto *const begin {display_name.data()};
      const auto *const end {display_name.data() + display_name.size()};
      const auto [ptr, ec] {std::from_chars(begin, end, display_id)};
      if (ec != std::errc {} || ptr != end) {
        return std::nullopt;
      }

      return display_id;
    }

    OSType videotoolbox_pixel_format(const video::config_t &config) {
      const auto colorspace {video::colorspace_from_client_config(config, false)};
      return colorspace.bit_depth == 10 ? kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange : kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
    }
  }  // namespace

  /**
   * @brief macOS display capture source and image buffers.
   */
  struct screen_display_t: public display_t {
    MusterScreenVideo *screen {};  ///< ScreenCaptureKit capture.
    CGDirectDisplayID display_id {};  ///< Display ID.
    std::unique_ptr<display_device::DisplayPowerGuardInterface> display_power_guard;  ///< Display power guard.

    int frame_rate = 0;

    ~screen_display_t() override {
      [screen stop];
      [screen release];
    }

    static void assign_frame(img_t &image, CMSampleBufferRef sample) {
      auto &frame = static_cast<av_img_t &>(image);
      auto owner = std::make_shared<av_sample_buf_t>(sample);
      auto pixels = std::make_shared<av_pixel_buf_t>(sample);
      frame.pixel_buffer = pixels;
      frame.sample_buffer = owner;
      auto buf = pixels->buf;
      const bool planar = CVPixelBufferIsPlanar(buf);
      image.data = planar ? static_cast<uint8_t *>(CVPixelBufferGetBaseAddressOfPlane(buf, 0)) : pixels->data();
      image.width = (int) CVPixelBufferGetWidth(buf);
      image.height = (int) CVPixelBufferGetHeight(buf);
      image.row_pitch = (int) (planar ? CVPixelBufferGetBytesPerRowOfPlane(buf, 0) : CVPixelBufferGetBytesPerRow(buf));
      image.pixel_pitch = image.row_pitch / image.width;
    }

    capture_e capture(const push_captured_image_cb_t &push, const pull_free_image_cb_t &pull, bool *cursor) override {
      bool login = muster_session::capture_login();
      if (![screen startLockedSurface:login]) return capture_e::error;
      auto stop = util::fail_guard([this] {
        muster_session::presented.store(muster_session::surface::none);
        [screen stop];
      });
      for (;;) {
        @autoreleasepool {
          const bool switching = login && !muster_session::capture_login();
          if (switching) {
            [screen stop]; // Discard buffered samples and detach the old delegate source.
            if (![screen startLockedSurface:NO]) return capture_e::error;
            login = false;
          }
          BOOL ended = NO;
          CMSampleBufferRef sample = [screen copyFrameWaiting:(switching ? 3.0 : 0.05) ended:&ended];
          if (switching && !sample) return capture_e::error;
          if (ended) {
            muster_session::presented.store(muster_session::surface::none);
            if (!login) return capture_e::error;
            // The login stream may stop just before Rust observes OS unlock.
            const auto until = std::chrono::steady_clock::now() + 1s;
            while (muster_session::capture_login() && std::chrono::steady_clock::now() < until) {
              std::this_thread::sleep_for(20ms);
            }
            if (muster_session::capture_login()) return capture_e::error;
            continue;
          }
          auto release = util::fail_guard([sample] { if (sample) CFRelease(sample); });
          std::shared_ptr<img_t> img;
          if (!pull(img)) return capture_e::ok;
          if (sample) assign_frame(*img, sample);
          if (!push(std::move(img), sample != nullptr)) return capture_e::ok;
          if (sample && muster_session::began_locked()) {
            muster_session::presented.store(login ? muster_session::surface::login : muster_session::surface::desktop);
          }
        }
      }
    }

    /**
     * @brief Allocate an image buffer compatible with this display backend.
     *
     * @return Allocated img object, or null when unavailable.
     */
    std::shared_ptr<img_t> alloc_img() override {
      return std::make_shared<av_img_t>();
    }

    /**
     * @brief Create AVCodec encode device.
     *
     * @param pix_fmt Sunshine pixel format to convert or allocate for.
     * @return Constructed AVCodec encode device object.
     */
    std::unique_ptr<avcodec_encode_device_t> make_avcodec_encode_device(pix_fmt_e pix_fmt) override {
      if (pix_fmt == pix_fmt_e::yuv420p) {
        screen.pixelFormat = kCVPixelFormatType_32BGRA;

        return std::make_unique<avcodec_encode_device_t>();
      } else if (pix_fmt == pix_fmt_e::nv12 || pix_fmt == pix_fmt_e::p010) {
        auto device = std::make_unique<nv12_zero_device>();

        device->init(static_cast<void *>(screen), pix_fmt, setResolution, setPixelFormat);

        return device;
      } else {
        BOOST_LOG(error) << "Unsupported Pixel Format."sv;
        return nullptr;
      }
    }

    /**
     * @brief Populate a fallback image when real capture data is unavailable.
     *
     * @param img Image or frame object to read from or populate.
     * @return Capture status reported to the streaming pipeline.
     */
    int dummy_img(img_t *img) override {
      // Encoder probes can run alongside the live capture on this display.
      auto *probe = [[MusterScreenVideo alloc] initWithDisplay:display_id frameRate:frame_rate];
      auto cleanup = util::fail_guard([probe] { [probe stop]; [probe release]; });
      probe.frameWidth = screen.frameWidth;
      probe.frameHeight = screen.frameHeight;
      probe.pixelFormat = screen.pixelFormat;
      if (![probe startLockedSurface:muster_session::capture_login()]) return 1;
      BOOL ended = NO;
      CMSampleBufferRef sample = [probe copyFrameWaiting:3 ended:&ended];
      if (!sample) return 1;
      assign_frame(*img, sample);
      CFRelease(sample);
      return 0;
    }

    /**
     * A bridge from the pure C++ code of the hwdevice_t class to the pure Objective C code.
     *
     * display --> an opaque pointer to an object of this class
     * width --> the intended capture width
     * height --> the intended capture height
     * @param display Display object or identifier associated with the operation.
     * @param width Frame or display width in pixels.
     * @param height Frame or display height in pixels.
     */
    static void setResolution(void *display, int width, int height) {
      auto *screen = static_cast<MusterScreenVideo *>(display);
      screen.frameWidth = width;
      screen.frameHeight = height;
    }

    /**
     * @brief Set pixel format.
     *
     * @param display Display object or identifier associated with the operation.
     * @param pixelFormat Pixel format.
     */
    static void setPixelFormat(void *display, OSType pixelFormat) {
      static_cast<MusterScreenVideo *>(display).pixelFormat = pixelFormat;
    }
  };

  std::shared_ptr<display_t> display(platf::mem_type_e hwdevice_type, const std::string &display_name, const video::config_t &config) {
    if (hwdevice_type != platf::mem_type_e::system && hwdevice_type != platf::mem_type_e::videotoolbox) {
      BOOST_LOG(error) << "Could not initialize display with the given hw device type."sv;
      return nullptr;
    }

    auto display = std::make_shared<screen_display_t>();

    BOOST_LOG(debug) << "Waking display for capture selector ["sv << display_name << ']';
    if (!display_device::wake_display(display_name, 1s)) {
      BOOST_LOG(debug) << "Display wake attempt did not expose the requested display ["sv << display_name << ']';
    }

    display->display_power_guard = display_device::keep_display_awake("Sunshine display capture");
    if (display->display_power_guard) {
      BOOST_LOG(debug) << "Keeping display awake for capture"sv;
    } else {
      BOOST_LOG(debug) << "Unable to create display sleep prevention assertion"sv;
    }

    // Default to main display
    display->display_id = CGMainDisplayID();

    if (const auto configured_display_id {parse_display_id(display_name)}) {
      display->display_id = *configured_display_id;
    } else if (!display_name.empty()) {
      BOOST_LOG(warning) << "Configured display ["sv << display_name
                         << "] is not a valid macOS capture display id. Falling back to main display ["sv
                         << display->display_id << "]."sv;
    }

    // Print all displays available with their names and ids
    BOOST_LOG(debug) << "Detecting displays"sv;
    for (const auto &device : display_device::enumerate_devices()) {
      if (device.m_display_name.empty()) {
        continue;
      }

      BOOST_LOG(debug) << "Detected display: "sv << device.m_friendly_name
                       << " (id: "sv << device.m_display_name << ") connected: true"sv;
    }

    BOOST_LOG(info) << "Configuring selected display ("sv << display->display_id << ") to stream"sv;

    display->frame_rate = config.framerate;
    display->screen = [[MusterScreenVideo alloc] initWithDisplay:display->display_id frameRate:config.framerate];

    if (!display->screen || (muster_session::capture_login() && !display->screen.lockSurfaceAvailable)) {
      BOOST_LOG(error) << "Video setup failed."sv;
      return nullptr;
    }

    display->width = display->screen.frameWidth;
    display->height = display->screen.frameHeight;
    // We also need set env_width and env_height for absolute mouse coordinates
    display->env_width = display->width;
    display->env_height = display->height;

    if (hwdevice_type == platf::mem_type_e::videotoolbox) {
      const auto pixel_format {videotoolbox_pixel_format(config)};
      display->screen.frameWidth = config.width;
      display->screen.frameHeight = config.height;
      display->screen.pixelFormat = pixel_format;
    }

    return display;
  }

  std::vector<std::string> display_names(mem_type_e hwdevice_type) {
    std::vector<std::string> display_names;
    if (hwdevice_type != platf::mem_type_e::system && hwdevice_type != platf::mem_type_e::videotoolbox) {
      return display_names;
    }

    const auto devices {display_device::enumerate_devices()};
    display_names.reserve(devices.size());
    for (const auto &device : devices) {
      if (!device.m_display_name.empty()) {
        display_names.emplace_back(device.m_display_name);
      }
    }

    return display_names;
  }

  /**
   * @brief Report whether encoder backends should be probed again before streaming.
   *
   * @return Always `true` because macOS GPU changes are not tracked by this backend.
   */
  bool needs_encoder_reenumeration() {
    // We don't track GPU state, so we will always reenumerate. Fortunately, it is fast on macOS.
    return true;
  }
}  // namespace platf
