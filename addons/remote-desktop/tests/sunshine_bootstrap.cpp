#include "../vendor/sunshine/src/muster_session.h"

#include <sstream>
#include <stdexcept>

static std::string frame(unsigned char mode, const std::string &pem) {
  std::string data = "MST1";
  data.push_back(static_cast<char>(mode));
  data.push_back(static_cast<char>(pem.size() >> 8));
  data.push_back(static_cast<char>(pem.size()));
  return data + pem;
}

#ifdef _WIN32
#include "../vendor/sunshine/src/platform/windows/muster_ordinary_desktop.h"
#endif

int main() {
#ifdef _WIN32
  if (muster_console::named(nullptr, L"Default") ||
      muster_console::named(GetProcessWindowStation(), L"MusterMissingStation")) {
    return 1;
  }
#endif
  const std::string sample = "-----BEGIN CERTIFICATE-----\nfixture\n-----END CERTIFICATE-----\n";
  for (unsigned char mode : {0, 1, 2}) {
    std::istringstream pipe(frame(mode, sample) + "X");
    auto grant = muster_session::read(pipe);
    if (!grant || grant->mouse != (mode != 0) || grant->keyboard != (mode == 2) || grant->certificate != sample || pipe.get() != 'X') {
      return 1;
    }
  }
  for (const auto &bad : {std::string(), std::string("MST2"), frame(3, sample),
                          frame(0, std::string(8193, 'a')), frame(0, sample + sample),
                          frame(0, sample).substr(0, 10), frame(0, "not a certificate")}) {
    std::istringstream pipe(bad);
    if (muster_session::read(pipe)) {
      return 1;
    }
  }
#ifdef __APPLE__
  setenv("MUSTER_DESKTOP_LOCK_SURFACE", "1", 1);
  using muster_session::surface;
  for (bool mouse : {false, true}) {
    muster_session::mouse_enabled.store(mouse);
    for (bool unlocked : {false, true}) {
      muster_session::unlocked_by_owner.store(unlocked);
      const auto requested = unlocked ? surface::desktop : surface::login;
      for (auto frame : {surface::none, surface::login, surface::desktop}) {
        muster_session::presented.store(frame);
        const auto expected = mouse && frame == requested ? frame : surface::none;
        if (muster_session::mouse_surface() != expected) return 1;
      }
    }
  }
  unsetenv("MUSTER_DESKTOP_LOCK_SURFACE");
  muster_session::presented.store(surface::none);
  if (muster_session::mouse_surface() != surface::desktop) return 1;
#endif
  return 0;
}
