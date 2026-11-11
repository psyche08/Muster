# ADR 0184: Keep the transcript views' memberwise initializers internal

- Date: 2026-11-11
- Status: Accepted; source verified and the iOS simulator build passes
- Related: architecture §3.7 (ViewModels are display-only), ADR 0160 (turn
  pages and on-demand work), ADR 0178 (consistent session list rows)

## Context

Building `MusteriOS` with Xcode 26.4 (Swift 6.3) fails with four errors in
`AgentScreens.swift`: the `WorkingGroup`, `ActivityGroup` and
`ToolActivityRow` initializers are "inaccessible due to 'private' protection
level" at their call sites in `ConversationBlock` and `WorkingGroup.rows`.
Three more transcript views with the same shape (`TurnChanges`,
`CompactContext`, `ReasoningRow`, `SessionNotice`, `SessionDiffCard`) fail the
same way once the first errors are fixed.

The synthesized memberwise initializer of a Swift struct takes the access
level of its least accessible stored property. The transcript views declare
`private var disclosure = TranscriptDisclosure()` (and, on some,
`private var expanded = false`): plain private stored properties with default
values. A minimal repro confirms Swift 6.3 synthesizes
`init(block:disclosure:)` as private for such a struct, so no other type —
not even one in the same file — can construct the view. `@Environment`,
`@State` and `@ScaledMetric` properties are initialized through their
wrappers at declaration and do not participate; `MessageBubble`, which has
only those, compiles.

Older toolchains excluded these private defaulted properties from the
synthesized initializer, so the same source compiled there (the 1.1.36
release built on Xcode Cloud). The repo does not pin an Xcode version for
local builds, so source that only compiles under the older rule breaks every
developer on the current Xcode.

## Decision

Drop `private` from the plain stored properties that do not participate in
any other initialization path: `disclosure` on the eight transcript views and
`expanded` on `TurnChanges`, `CompactContext` and `SessionNotice`
(`AgentScreens.swift`, `SessionPresentation.swift`). The synthesized
memberwise initializers become internal, and every existing call site
compiles unchanged. The fields remain view-internal state by convention; no
caller passes them. The comment on `WorkingGroup.disclosure` records why the
field is not private.

Behavior is unchanged: the properties keep their defaults, and the views'
bodies, access modifiers on computed properties, and `@Environment` reads are
untouched.

## Alternatives

- Explicit initializers on each of the eight structs preserve full
  encapsulation but add boilerplate that must track future stored properties;
  the existing explicit inits in this target (for example `AgentsView`,
  `ChatView`) exist for different reasons.
- Pinning developers to the older Xcode keeps the source untouched but
  contradicts the repo's small-change defaults and cannot hold as Xcode Cloud
  eventually moves forward.
- Switching `TranscriptDisclosure` to an `@Environment`-style wrapper does
  not apply: it is a `DynamicProperty` view state, not an environment read.

## Consequences

- Every transcript view can be constructed from anywhere in the module; the
  exposed `disclosure` and `expanded` parameters have defaults, so call sites
  stay as they are.
- The same pattern must not be reintroduced: a plain private stored property
  on a view struct constructed outside its own declaration silences the
  initializer again.
- No pin on the build toolchain is added; the fix is compatible with both the
  old and the new memberwise-initializer rules.

## Verification

- Source: minimal `swiftc -typecheck` repro reproduced the failure and the
  synthesized `init(a:disclosure:)` signature; the edited files no longer
  declare the pattern.
- Build: `xcodebuild -scheme MusteriOS -destination 'generic/platform=iOS
  Simulator' ARCHS=arm64` against `libmuster_ffi.a` built from `muster-ffi`
  (release, arm64 sim) ends in BUILD SUCCEEDED. Without `ARCHS=arm64` the
  build still compiles every Swift source but cannot link, because the
  generic simulator destination also builds an x86_64 slice and the Rust
  workspace builds only the host arch here.
- Runtime and device acceptance are unaffected: no rendering or state change
  is intended, and the transcript acceptance of ADR 0160 / 0178 stands.
