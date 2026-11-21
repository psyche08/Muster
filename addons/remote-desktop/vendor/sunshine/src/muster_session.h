// Private inherited-pipe bootstrap for the downstream Muster child.
#pragma once

#include <array>
#include <atomic>
#include <istream>
#include <optional>
#include <string>
#include <utility>

namespace muster_session {
  struct grant {
    bool mouse;
    std::string certificate;
  };

  // MST1, one host-selected access byte (0 view, 1 mouse), two bytes of
  // big-endian PEM length, then exactly that many bytes. The remainder of
  // the pipe is a lifetime signal, never another grant.
  inline std::optional<grant> read(std::istream &pipe) {
    std::array<unsigned char, 7> header {};
    if (!pipe.read(reinterpret_cast<char *>(header.data()), header.size()) ||
        header[0] != 'M' || header[1] != 'S' || header[2] != 'T' || header[3] != '1' || header[4] > 1) {
      return std::nullopt;
    }
    const size_t length = (size_t(header[5]) << 8) | header[6];
    if (length == 0 || length > 8192) {
      return std::nullopt;
    }
    std::string pem(length, '\0');
    if (!pipe.read(pem.data(), pem.size()) || !pem.starts_with("-----BEGIN CERTIFICATE-----\n") ||
        !pem.ends_with("-----END CERTIFICATE-----\n") ||
        pem.find("-----BEGIN", 1) != std::string::npos || pem.find('\0') != std::string::npos) {
      return std::nullopt;
    }
    return grant {header[4] == 1, std::move(pem)};
  }

  inline std::atomic<bool> mouse_enabled {false};
}
