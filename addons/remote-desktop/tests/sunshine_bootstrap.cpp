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

int main() {
  const std::string sample = "-----BEGIN CERTIFICATE-----\nfixture\n-----END CERTIFICATE-----\n";
  for (unsigned char mode : {0, 1}) {
    std::istringstream pipe(frame(mode, sample) + "X");
    auto grant = muster_session::read(pipe);
    if (!grant || grant->mouse != (mode == 1) || grant->certificate != sample || pipe.get() != 'X') {
      return 1;
    }
  }
  for (const auto &bad : {std::string(), std::string("MST2"), frame(2, sample),
                          frame(0, std::string(8193, 'a')), frame(0, sample + sample),
                          frame(0, sample).substr(0, 10), frame(0, "not a certificate")}) {
    std::istringstream pipe(bad);
    if (muster_session::read(pipe)) {
      return 1;
    }
  }
  return 0;
}
