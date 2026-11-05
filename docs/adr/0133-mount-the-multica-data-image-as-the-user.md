# ADR 0133: Mount the Multica data image as the user, not through diskutil

- Date: 2026-11-05
- Status: Accepted; source and tests verified, install acceptance passed on
  one Mac from the branch build
- Related: [ADR 0130](0130-mount-the-multica-data-image-before-the-container-starts.md)
  (this record replaces its mount step and leaves the rest of it in place)

## Context

ADR 0130 has musterd mount `<root>.sparsebundle` before it starts a stopped
Multica container. It attaches the image with `hdiutil attach -nomount`, then
mounts the APFS volume with `diskutil mount nobrowse -mountPoint`. That
sequence had worked from an interactive shell. It failed the first time the
installed daemon ran it.

The install acceptance stopped the container, then unmounted and detached the
image. It left the empty mount-point directory in place, which is the case ADR
0130 exists for. It then turned keep-alive back on. musterd attached the image,
and its `diskutil mount` call never returned. The system logs show the
following sequence:

1. Disk Arbitration refused the mount request as not privileged (status
   `0xF8DA0009`).
2. StorageKit then asked authd for the right
   `com.apple.DiskManagement.root.SKRequest`, which requires an administrator.
3. SecurityAgent put a password prompt on screen.

Each later keep-alive pass started another `diskutil mount`. These calls queued
in storagekitd behind the prompt and were killed at the 60-second timeout. The
queue also blocked other `diskutil` operations on the Mac, including an
unrelated `diskutil unmount`. A daemon that starts the container unattended,
for example after a reboot, cannot depend on someone answering a password
prompt.

The image is attached by the same user that runs musterd. That user therefore
owns the volume's device nodes, and also owns the mount point. On macOS that is
enough for an ordinary user mount, which the kernel restricts to `nosuid` and
`nodev`. `/sbin/mount_apfs -o nobrowse,noowners <device> <root>` was tested on
a scratch image and on the real data image. Both mounts succeeded at once with
the same flags that Disk Arbitration had applied: `nobrowse`, `noowners`, and
"mounted by" the user. They involved no StorageKit and no prompt. Unmounting
with `umount` and detaching with `hdiutil detach` also needed neither.

This record does not establish why Disk Arbitration refused the daemon but
accepted the interactive shell. A difference in session is the likely cause.
The decision does not depend on that explanation.

## Decision

`multica::image::prepare` mounts the attached APFS volume with
`/sbin/mount_apfs -o nobrowse,noowners <device> <root>`, with a 30-second
bound, instead of `diskutil mount`. The other parts of ADR 0130 stay as they
were:

- The image beside the root is the only setting.
- A root that is already a mount point is skipped.
- A root that holds files of its own is refused.
- Attachment uses `hdiutil attach -nomount`.
- After a failed mount, only a mount point that `prepare` created is removed.

## Alternatives

- **Keep `diskutil mount` and have the operator approve once.** The approval
  is not remembered for the next attach, and the prompt blocks every other
  `diskutil` call while it waits.
- **Mount through the root helper.** musterd already has a privileged helper
  for Locked Use, but a mount that the user can do needs no privileged path.
- **Let Disk Arbitration mount at `/Volumes/<name>` and link the root to it.**
  The host-native runtimes and the container would then see different
  canonical paths for the same tree.

## Consequences

- An unattended keep-alive pass can mount the image. No password prompt can
  stall it, and it does not depend on storagekitd.
- Disk Arbitration still sees the volume. `diskutil unmount` and `umount`
  both work, as before.
- The mount depends on the user owning the attached device nodes. An image
  attached by another user, for example root, fails to mount, and the error is
  reported through keep-alive's last error, as before.

## Verification

- Source and tests: `cargo clippy -p musterd --tests` is clean, and the Multica
  tests in `cargo test -p musterd` pass, 35 in all. The fakes now stand in for
  `mount_apfs`. The keep-alive test still checks two things: that `hdiutil`
  runs, then `mount_apfs`, then `container start`; and that a failed mount
  starts nothing.
- Manual sequence:
  - `mount_apfs -o nobrowse,noowners` mounted a scratch case-sensitive APFS
    sparsebundle and the real data image as the user.
  - On the real image, the next keep-alive pass found the root mounted,
    skipped the mount, started the container, and brought the Multica daemon
    up.
- Install acceptance, passed. A build of this branch was installed as 1.1.21
  (111) on the Mac where ADR 0130 failed.
  - Through a request: with keep-alive off, the container was stopped, the
    volume unmounted with `umount`, and the image detached. An empty mount
    point was left behind. Turning keep-alive on returned within 2 seconds.
    By then the image was mounted with `nobrowse` and `noowners` as the user,
    the container was running, and the daemon reported running.
  - Through keep-alive alone: with keep-alive on, the same teardown was done
    and no request was sent. The next pass mounted the image and started the
    container within 8 seconds.
  - Neither run showed a password prompt. The host and the guest listed the
    same entries in the data root.
- Pending: reinstalling from main after merge. A real reboot has not been
  tested; the keep-alive-alone run is its closest stand-in.
