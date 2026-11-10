# ADR 0169: Package the session list refresh change for desktop acceptance

- Date: 2026-11-10
- Status: Accepted; installation and device evidence pending
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
