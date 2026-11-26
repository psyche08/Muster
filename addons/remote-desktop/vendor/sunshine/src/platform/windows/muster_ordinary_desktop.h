/**
 * @file muster_ordinary_desktop.h
 * @brief Read-only checks for the physical user's ordinary desktop.
 */
#pragma once
#include <windows.h>
#include <cwchar>

namespace muster_console {
  /** @brief Check an OS window-station or desktop name, failing closed on query errors. */
  inline bool named(HANDLE object, const wchar_t *expected) {
    wchar_t name[64] {};
    DWORD bytes = 0;
    return object && GetUserObjectInformationW(object, UOI_NAME, name, sizeof(name), &bytes) &&
           bytes >= sizeof(wchar_t) && bytes <= sizeof(name) && name[bytes / sizeof(wchar_t) - 1] == 0 &&
           _wcsicmp(name, expected) == 0;
  }

  /** @brief Verify the current thread and input desktop without attaching to another desktop. */
  inline bool ordinary() {
    DWORD session = 0;
    if (!ProcessIdToSessionId(GetCurrentProcessId(), &session) || !session || session != WTSGetActiveConsoleSessionId() ||
        !named(GetProcessWindowStation(), L"WinSta0") || !named(GetThreadDesktop(GetCurrentThreadId()), L"Default")) {
      return false;
    }
    HDESK input = OpenInputDesktop(0, FALSE, DESKTOP_READOBJECTS);
    if (!input) {
      return false;
    }
    const bool allowed = named(input, L"Default");
    CloseDesktop(input);
    return allowed;
  }
}
