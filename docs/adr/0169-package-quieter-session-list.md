# ADR 0169: Package the session list refresh change for desktop acceptance

- Date: 2026-11-10
- Status: Accepted; three desktop installations and live measurements verified
- Related: ADR 0144, ADR 0166, ADR 0168

## Context

The operator requests installation of ADR 0168 on three Macs and observation of
the session list scrollbar and CPU usage on the running applications.

## Decision

Use Apple version 1.1.29 (119) for the new source. Build one Developer ID signed,
notarized portable for the three named desktop installations. Verify the build
stamp, signature, package hash and installed daemon on each device. Use the
existing update lease and the independent installation procedure from ADR 0144.

## Alternatives

Reusing 1.1.28 would give distinct binaries the same version. Replacing just the
daemon would invalidate the bundle's code signature and would skip the normal
application startup path.

## Consequences

All installed source hosts can coalesce their catalog notifications. A five
second interval per source does not establish stability of the combined list
or a particular CPU load on a live device.

## Verification

Compare the old and new foreground application and daemon CPU over timed
windows. Scroll within the local list, stop input, and observe its position
over multiple refresh intervals. Record runtime conditions and keep the
installation, visual observations, and CPU evidence separate.

## Desktop acceptance on 2026-11-10

Built 1.1.29 (119) from clean merge commit
`75210e10bc4fa121f35378968dbe3ae56cd61533`. The packaged daemon passed
the release ancestry and build-stamp checks. Developer ID and Presence signing,
Apple notarization, stapling, strict signature verification and Gatekeeper
assessment passed. The three independent installers verified the same portable
SHA-256 (`abdab544ba9c570b9ee04a34035a824bd9e1dbe47cdc850ff8cdaad15a6f7325`)
and installed daemon SHA-256
(`bb7aab9b5155a43d828e193b18c0f1681a2f5ced41c47cbc5e1dc23954575eb1`).
All three live daemons reported the expected version and commit, retained their
device identities, and reported Accessibility granted, Relay registered and Vnet up.
The scheduler on the host with a running managed container recovered.

On the observation desktop, the real native session outline had 98 rows.
Accessibility samples recorded the normalized vertical scrollbar value and
first visible row once per second. On the old build, one 40-second observation
went from row index 33 to 0 and a repeat kept index 33. There was no
system-wide input trace for the return to the top. On the new build three
40-second observations starting at 45%, 65% and 32% kept first visible indices
35, 49 and 24 respectively. The final run also retained the same anonymous
first-row content fingerprint. The scrollbar thumb still moved: the largest
within-run range was 2.37 percentage points, with the first row's vertical
coordinate spanning up to 4.3 points. Long-running stability remains unmeasured.

CPU percentages are process CPU-time deltas over about 30 seconds per host
per version, where 100% is one logical core. Each row uses an anonymized host.
The installations restarted the applications, and workloads were live.

| Desktop | Old daemon | New daemon | Old app | New app |
| --- | ---: | ---: | ---: | ---: |
| Observation desktop | 76.9% | 30.5% | 13.3% | 3.3% |
| Peer B | 83.0% | 17.9% | App absent | 1.9% |
| Peer C | 44.2% | 15.7% | 1.8% | 1.9% |

Delivery covered direct desktop installation. The public feeds and mobile
distribution received no publication during this acceptance.
