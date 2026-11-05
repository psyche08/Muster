//! Prepare stage: mount the data root's disk image before the container starts.
use super::{Config, Manager, io, run};
use crate::Error;
use std::path::{Path, PathBuf};

/// The APFS volume type that `hdiutil attach` lists (truncated in its table).
const APFS_VOLUME: &str = "41504653-0000-11AA-AA11";
/// The placeholder Disk Arbitration may leave in a mount point it created.
const PLACEHOLDER: &str = ".autodiskmounted";

/// A data root backed by an image keeps it beside the root as
/// `<root>.sparsebundle`.
fn image_for(root: &Path) -> PathBuf {
    let mut name = root.as_os_str().to_owned();
    name.push(".sparsebundle");
    PathBuf::from(name)
}

/// Whether `path` is the root of a mounted file system.
#[cfg(unix)]
fn is_mount_point(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (
        std::fs::metadata(path),
        path.parent().map(std::fs::metadata),
    ) {
        (Ok(own), Some(Ok(parent))) => own.dev() != parent.dev(),
        _ => false,
    }
}

/// The device node of the APFS volume in `hdiutil attach` output.
fn apfs_volume(listing: &[u8]) -> Option<String> {
    String::from_utf8_lossy(listing).lines().find_map(|line| {
        let mut fields = line.split('\t').map(str::trim);
        let device = fields.next()?;
        fields
            .next()?
            .starts_with(APFS_VOLUME)
            .then(|| device.to_owned())
    })
}

#[cfg(not(test))]
fn tool(_: &Manager, path: &str) -> PathBuf {
    PathBuf::from(path)
}
#[cfg(test)]
fn tool(manager: &Manager, path: &str) -> PathBuf {
    let name = Path::new(path).file_name().unwrap();
    manager
        .disk_tools
        .as_ref()
        .map_or_else(|| PathBuf::from(path), |dir| dir.join(name))
}

/// Mount `<root>.sparsebundle` at the data root before the container starts, so
/// its bind mount sees the image and not an empty directory. A root without an
/// image beside it, or one that is already mounted, is left alone, and a root
/// holding files of its own is never hidden under the mount.
#[cfg(unix)]
pub(super) async fn prepare(manager: &Manager, c: &Config) -> Result<(), Error> {
    let root = Path::new(&c.data_dir);
    let image = image_for(root);
    if !image.is_dir() || is_mount_point(root) {
        return Ok(());
    }
    if root.exists() {
        let occupied = std::fs::read_dir(root)
            .map_err(io)?
            .filter_map(Result::ok)
            .any(|entry| entry.file_name() != PLACEHOLDER);
        if occupied {
            return Err(Error::Conflict(format!(
                "{} holds files but its data image is not mounted; move them before Multica starts",
                root.display()
            )));
        }
    }
    let attach_failed = || {
        Error::Unavailable(format!(
            "Could not attach the Multica data image {}",
            image.display()
        ))
    };
    let attached = run(
        &tool(manager, "/usr/bin/hdiutil"),
        &["attach", "-nomount", &image.to_string_lossy()],
        60,
    )
    .await
    .map_err(|_| attach_failed())?;
    let device = apfs_volume(&attached).ok_or_else(attach_failed)?;
    let created = match std::fs::create_dir(root) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(e) => return Err(io(e)),
    };
    // Mount as the user who attached the image, which owns its device nodes
    // and the mount point. `diskutil mount` goes through Disk Arbitration,
    // which refuses a caller outside the console session as not privileged;
    // StorageKit then asks for an administrator password and every later
    // diskutil call queues behind that prompt.
    let mounted = run(
        &tool(manager, "/sbin/mount_apfs"),
        &["-o", "nobrowse,noowners", &device, &root.to_string_lossy()],
        30,
    )
    .await;
    if mounted.is_err() {
        if created {
            let _ = std::fs::remove_dir(root);
        }
        return Err(Error::Unavailable(format!(
            "Could not mount the Multica data image at {}; it may be mounted elsewhere",
            root.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn prepare(_: &Manager, _: &Config) -> Result<(), Error> {
    Ok(())
}

/// Stand-ins for hdiutil and mount_apfs that log to `<dir>/calls`.
#[cfg(all(test, unix))]
pub(super) fn fake_tools(dir: &Path, mount_status: i32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let calls = dir.join("calls");
    for (name, body) in [
        (
            "hdiutil",
            "printf '/dev/disk9          \\tGUID_partition_scheme          \\t\\n/dev/disk10         \\tEF57347C-0000-11AA-AA11-0030654\\t\\n/dev/disk10s1       \\t41504653-0000-11AA-AA11-0030654\\t\\n'"
                .to_owned(),
        ),
        ("mount_apfs", format!("exit {mount_status}")),
    ] {
        let path = bin.join(name);
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '{name} %s\\n' \"$*\" >> '{}'\n{body}\n",
                calls.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    bin
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn setup(mount_status: i32) -> (tempfile::TempDir, Manager, Config) {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = Manager::new(dir.path());
        manager.disk_tools = Some(fake_tools(dir.path(), mount_status));
        let c = Config {
            data_dir: dir.path().join("Multica").to_string_lossy().into_owned(),
            ..Default::default()
        };
        (dir, manager, c)
    }

    fn calls(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("calls")).unwrap_or_default()
    }

    #[test]
    fn reads_the_apfs_volume_and_mount_points() {
        let listing = b"/dev/disk10         \tEF57347C-0000-11AA-AA11-0030654\t\n/dev/disk10s1       \t41504653-0000-11AA-AA11-0030654\t/Volumes/Multica\n";
        assert_eq!(apfs_volume(listing).as_deref(), Some("/dev/disk10s1"));
        assert_eq!(apfs_volume(b"/dev/disk9\tGUID_partition_scheme\t\n"), None);
        assert_eq!(
            image_for(Path::new("/Volumes/Data/Multica")),
            Path::new("/Volumes/Data/Multica.sparsebundle")
        );
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_mount_point(dir.path()));
        assert!(!is_mount_point(&dir.path().join("missing")));
        assert!(is_mount_point(Path::new("/dev")));
    }

    #[tokio::test]
    async fn a_root_without_an_image_is_left_alone() {
        let (dir, manager, c) = setup(0);
        prepare(&manager, &c).await.unwrap();
        assert!(calls(dir.path()).is_empty());
        assert!(!Path::new(&c.data_dir).exists());
    }

    #[tokio::test]
    async fn the_image_is_attached_and_mounted_at_the_root() {
        let (dir, manager, c) = setup(0);
        std::fs::create_dir(image_for(Path::new(&c.data_dir))).unwrap();
        prepare(&manager, &c).await.unwrap();
        assert_eq!(
            calls(dir.path()),
            format!(
                "hdiutil attach -nomount {0}.sparsebundle\nmount_apfs -o nobrowse,noowners /dev/disk10s1 {0}\n",
                c.data_dir
            )
        );
        assert!(Path::new(&c.data_dir).is_dir());
    }

    #[tokio::test]
    async fn a_failed_mount_removes_only_the_mount_point_it_created() {
        let (dir, manager, c) = setup(1);
        let root = Path::new(&c.data_dir);
        std::fs::create_dir(image_for(root)).unwrap();
        assert!(prepare(&manager, &c).await.is_err());
        assert!(!root.exists());
        std::fs::create_dir(root).unwrap();
        std::fs::write(root.join(PLACEHOLDER), b"").unwrap();
        assert!(prepare(&manager, &c).await.is_err());
        assert!(root.join(PLACEHOLDER).exists());
        assert_eq!(calls(dir.path()).lines().count(), 4);
    }

    #[tokio::test]
    async fn files_in_an_unmounted_root_are_never_hidden() {
        let (dir, manager, c) = setup(0);
        let root = Path::new(&c.data_dir);
        std::fs::create_dir(image_for(root)).unwrap();
        std::fs::create_dir(root).unwrap();
        std::fs::write(root.join("bridge-token"), b"stale").unwrap();
        assert!(matches!(
            prepare(&manager, &c).await,
            Err(Error::Conflict(_))
        ));
        assert!(calls(dir.path()).is_empty());
    }
}
