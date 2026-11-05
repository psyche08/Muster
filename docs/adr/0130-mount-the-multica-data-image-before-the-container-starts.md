# ADR 0130: Mount the Multica data image before the container starts

- Date: 2026-11-05
- Status: Accepted; mount step superseded by
  [ADR 0133](0133-mount-the-multica-data-image-as-the-user.md) after the
  install acceptance failed
- Related: [ADR 0133](0133-mount-the-multica-data-image-as-the-user.md)
  (replaces the `diskutil mount` step), [ADR 0051](0051-multica-management-and-live-catalog.md) (musterd
  owns the container lifecycle), [ADR 0052](0052-targeted-multica-deployment.md)
  (the data root can be relocated), [ADR 0064](0064-multica-native-runtime.md)
  (host-native runtimes open task directories under the same root)

## Context

Repositories that the Multica daemon clones can hold refs that differ only in
case. On the default case-insensitive APFS volume those clones break, so an
operator can put the data root on a case-sensitive APFS disk image instead. A
Linux file system such as ext4 would not do: the host-native runtimes behind
the Muster bridge open the task's working directory on the host, at the same
path the guest sees.

macOS does not mount such an image after a reboot. The container bind-mounts
the data root, so if musterd's keep-alive starts the container first, one of
two things goes wrong. If the mount point is gone, `container start` fails on
the missing source. If an empty directory is left there, the guest starts on
it, and the daemon writes a fresh state that is later hidden under the image.

Mounting the image has to happen before the container starts. A separate login
item could do it, but it would race musterd's first keep-alive pass and would be
one more thing to install by hand. musterd already starts the container, so it
can mount the image as the start's first step.

Only one mounting sequence works for a mount point under an external volume.
`hdiutil attach -mountpoint` there fails with "Permission denied", and
`diskutil image attach --mountPoint` hangs. What works is attaching without
mounting, then mounting the APFS volume on an existing directory with
`diskutil mount nobrowse -mountPoint`. A mount point with mode 000 also makes
`diskutil mount` hang, so the directory has to be an ordinary one.

## Decision

A data root is backed by an image when `<data root>.sparsebundle` exists beside
it. Nothing is recorded in `multica.json`: the image's presence is the setting,
so an operator turns it on by creating the image, not by editing configuration.

Before musterd starts a stopped container, and before an installation prepares
its data directory, `multica::image::prepare` does the following:

1. Leaves the root alone if no image is beside it, or if the root is already a
   mount point. A root is a mount point when its device differs from its
   parent's.
2. Refuses with a conflict if the unmounted root holds files. The only
   exception is the `.autodiskmounted` placeholder that Disk Arbitration can
   leave behind. A mount must never hide files, which in this state are most
   likely fresh daemon state written while the image was missing.
3. Runs `/usr/bin/hdiutil attach -nomount <image>` and takes the device whose
   partition type is the APFS volume type, `41504653-0000-11AA-AA11`.
4. Creates the root if it is missing, then runs
   `/usr/sbin/diskutil mount nobrowse -mountPoint <root> <device>`. If the
   mount fails, it removes only a directory it created itself.

Any failure stops the start. The error is recorded as the keep-alive's last
error, and the next pass tries again. A container that is already running is
not touched, so a mounted image is never remounted and a running guest never
loses its bind source. On platforms other than Unix the step does nothing.

## Alternatives

- **A user LaunchAgent that mounts the image at login.** It works without
  musterd changes, but it races keep-alive. It also adds a persistent item for
  the operator to install, and that item can fail without being seen.
- **Record an image path in `multica.json`.** This is more explicit, but the
  only value that makes sense is the sibling path, and a new field would need
  UI and migration for no gain.
- **ext4 or another Linux file system mounted only in the guest.** It is case
  sensitive, but the host-native runtimes could not open task directories on
  the host.
- **Mount with `hdiutil attach -mountpoint`.** It is simpler, but it fails
  under an external volume's root, as noted in Context.

## Consequences

- A data root on an image survives a reboot without anything to install by
  hand: the first keep-alive pass mounts it and starts the container.
- If the image cannot be attached or mounted, for example because it is
  mounted somewhere else or the volume holding it is missing, keep-alive
  reports the error instead of starting the guest on an empty directory.
- Files left in the unmounted root block the start until the operator moves
  them. This is deliberate. Merging them into the image automatically could
  overwrite the daemon's real state.
- musterd does not detach the image when the container stops. Unmounting stays
  a manual operation.
- The container runtime still needs the operator's grant for removable volumes
  before it can open a bind source on an external disk. This record does not
  change that prerequisite.

## Verification

- Source and tests: `cargo clippy -p musterd --tests` is clean, and
  `cargo test -p musterd` passes. New tests cover the following:
  - Parsing `hdiutil` output.
  - Detecting a mount point.
  - A root without an image that is left alone.
  - The attach-then-mount order.
  - A failed mount that removes only a mount point that `prepare` created.
  - An occupied root that is refused without attaching.
  - Keep-alive running `hdiutil` and then `diskutil` before `container start`,
    and not starting the container at all when the mount fails.
- Manual sequence: on one Mac, `hdiutil attach -nomount` followed by
  `diskutil mount nobrowse -mountPoint` mounted a case-sensitive APFS
  sparsebundle under an external volume. The container started on it once the
  runtime had the removable-volume grant. This was done by hand before this
  change, not by musterd.
- Pending: installing this musterd build, and a reboot or unmount test showing
  that keep-alive mounts the image and starts the container by itself.
- Install acceptance, failed: this build was installed from main as 1.1.21
  (111). The test stopped the container, unmounted and detached the image, and
  turned keep-alive back on.
  - musterd attached the image.
  - Its `diskutil mount` was refused by Disk Arbitration as not privileged.
    StorageKit then asked for an administrator password.
  - Every later pass queued behind the prompt until its 60-second timeout.
  - The container stayed stopped until the volume was mounted by hand.
  - [ADR 0133](0133-mount-the-multica-data-image-as-the-user.md) replaces the
    mount step.
