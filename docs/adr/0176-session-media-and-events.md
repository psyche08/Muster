# ADR 0176: Display provider images and recorded session events

- Date: 2026-11-10
- Status: Accepted
- Related: [history costs](0153-large-session-history-costs.md), [Claude cards](../CLAUDE_CARDS.md)

## Context

Claude and Codex embed images in conversation records. Codex also records file
references, and DeepSeek records an attachment digest. Some provider events and
image-only turns disappear during transcript projection.

## Decision

Shared Rust resolves recorded sources, bounds input and decoded dimensions and
projects small JPEG previews in the history wire message. Apple, Android and HarmonyOS
display these bytes inline. A missing source leaves a readable description.
DeepSeek objects must match the recorded SHA-256 digest. Codex file references
are limited to the session directory and native clipboard captures. A transcript
URL never initiates a network request. History renders model fallback, handoff
and provider activity as recorded information.

## Alternatives

Sending original uploads would increase page and cache sizes. Client-side path
resolution would require access to another device's filesystem.

## Consequences

Previews can be displayed on the receiving device using its existing session
access. Older clients can read the text description. Archived paths may have
expired. A thumbnail is for reading the conversation, not original-file delivery.

## Verification

Exercise embedded images, digest-backed attachments, confined paths, malformed
inputs, and event replay using synthetic records. Verify timeline presentation,
generated bindings and the Apple builds. Record live device coverage separately.

The three local provider histories each produced valid JPEG previews in
read-only replay. Synthetic fixtures cover digest mismatch, missing files,
symlink escape, image-only messages and cursor continuity over pages of images.
The macOS rendering test sampled the displayed pixels at phone and desktop
widths. The iOS Simulator UI target compiled. No physical device acceptance
or distribution was performed.

ArkTS compiled and the unsigned HarmonyOS HAP assembled with SDK 26 using
the temporary link workaround documented in ADR 0160. The unmodified native
link still encounters the existing errno symbol mismatch. Android Gradle
verification could not configure the project because the Android SDK was
absent on this host.
