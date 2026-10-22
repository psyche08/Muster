# Mac startup setup

The local Shared Rust core projects a startup sheet when required setup is
missing. It can be dismissed for the current app run and reopened from network
settings. Returning to the permission sheet triggers an immediate local query;
Device Watch also refreshes observations; completing setup closes the sheet.
Phone and non-macOS clients never show this local permission sheet.

- **Accessibility:** enable `musterd` at
  `/Applications/Muster.app/Contents/MacOS/musterd` in Privacy & Security →
  Accessibility. The existing local-only RPC requests the system permission.
- **Locked Use:** the application registers its root helper. The setup button
  opens Login Items so the user can approve the service. The daemon's existing
  bounded installation retry calls that helper; no separate installer or shell
  sudo prompt is introduced. Readiness is displayed only after the helper has
  installed and verified the plugin. Registration alone is not installation.
- **iTerm2:** the daemon observes Apple Events permission without prompting.
  Only a local setup action may request consent and check the existing iTerm2
  API connection. Start iTerm2, enable Settings → General → Magic → Python API,
  and approve Muster's first connection. A socket observation means the API is
  available, not that an authenticated connection has succeeded. The hardened
  daemon and application carry the Apple Events entitlement.

These fields are display observations, not new authorization inputs. Existing
Locked Use, Accessibility, local-only RPC and terminal authorization checks
remain responsible for operations. Real system permission dialogs require the
user's action; rendering and state tests do not establish successful consent.

The macOS asset catalog uses the existing vector icon and the iOS light/dark
accent colors. Regenerate it with `swift scripts/generate-mac-assets.swift`
from `src/native/apple/MusterApp`.
