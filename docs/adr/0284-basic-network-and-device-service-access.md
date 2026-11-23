# ADR 0284: Basic network grants and device-owned service access

- Date: 2026-11-23
- Status: Accepted for implementation; production rollout tracked separately
- Related: [Identity and Trust](../architecture/identity-and-trust.md), [Network Transport](../architecture/network.md), [ADR 0161](0161-multica-task-session-isolation.md)

## Context

Port grants are stored on every accessing member and repeated in the receiving
device's open-port policy. A device owner can disable Agents or Terminals but
cannot independently choose which members can use those services or their
parts. A paired network needs a coarse default and optional detailed rules.

## Decision

Basic is the default network mode. Its member grant is the existing signed
`net:*` scope; the owner chooses whether the receiving device offers the
network service. Advanced lets the administrator grant protocol and port
scopes and lets the receiving device narrow access by the same dimensions.
Members with an existing coarse grant can keep it in Advanced. A mode change
preserves stored detailed rules and access granted to every unrelated service.

Legacy rosters without an explicit mode present Advanced if a member has
effective port-specific grants and Basic otherwise. Adding the existing
`net:*` scope to every current member is compatible with installed versions.
An explicit mode choice is an admin-signed roster entry. Before authoring
one, the daemon uses the authenticated roster compatibility service to require
support from all current members, including phones. The resulting entry is
durable before broadcast and the checked roster is compared again before
publication. A pairing stream advertises support before an explicit-mode
network can approve its device. The format of earlier entries and the
network identity are unchanged.

A receiving device stores its own incoming rules locally. Agents and
Terminals can be disabled or given to all paired members or assigned members.
Agent rules can select a provider. Terminal rules can select view or control.
Network rules can select a whole service or, in Advanced, a protocol and port.
Assignments use stable member IDs. The target verifies the active member and
its own applicable rules in Rust. When configured, the target decides Agent
and Terminal access; network access still requires the signed network grant.
The existing lock, provider identity, presence and independent service checks
continue to apply. Device policy changes come from a local owner endpoint.

Existing device policies that have not opted into the service rules retain
their Agent, Terminal and open-port behavior. Enabling the whole network
service is a local choice. Advanced applies the target's open-port list
even when the audience is all paired members. The vnet packet filter and
forwarding service use the same decision in Shared Rust. Existing inbound
network flows are rechecked after a policy change.

## Alternatives

A wildcard applied to every target port policy during upgrade would expose
listeners the owner had kept closed. Using build version claims to decide
whether a peer can decode signed entries would leave mixed-version rosters
vulnerable to a partial upgrade. A UI-only setting would disagree with packet,
forwarding and daemon RPC authorization.

## Consequences

Existing hosts can adopt the new rules individually. Applying explicit
network modes needs a compatible connected roster. Basic authorization
covers TCP and UDP; application-level authentication remains in effect.
Phones still use TCP forwarding for supported in-app access.

The Settings network mode is administered by a network admin. In a desktop's
Settings, the Access to this device editor changes the local policy.
`muster network-access basic` and `muster network-access advanced` select the
mode. `muster policy --service-grants FILE` reads a JSON object containing the
`agents`, `terminals` and `network` grant sets. An omitted category uses the
earlier authorization. Each grant set has `all_devices` and a `rules` array of
`scope` and `device_ids` entries. `muster policy --offer-network true` enables
the local Network service.

## Verification

Cover the legacy migration, the signed mode encoding, scoping by membership
and service, provider and terminal selectors, Basic versus Advanced port
decisions, and policy revocation in Rust. Verify the Apple settings and CLI
against the same local policy and signed-mode RPCs. Record live roster
migration, builds, signing, installation, and device runtime separately.

Local verification covers the Rust core, packet filter, app core and paired
daemon access suite. The paired test proves that a device owner can assign a
provider independently of roster grants, remote admins cannot rewrite the
device policy, and an offline member blocks a new signed mode. Apple macOS
and iOS Simulator settings, Android Kotlin and the Windows app compile.
Production devices using the earlier binaries can synchronize the compatible
coarse network grants. Signing, publication and installation of the new
binaries remain pending.

HarmonyOS ArkTS also compiles in an unsigned UI-only configuration. The
existing shadowing of the ArkUI `enabled` method in the network settings
component was corrected to complete this check.
