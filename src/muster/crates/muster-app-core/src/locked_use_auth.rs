//! Requester-local macOS preference. A marker outside the evictable view cache
//! records opt-in. If storage cannot be inspected, keep the prompt enabled.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) struct Preference {
    path: PathBuf,
    required: bool,
}

impl Preference {
    pub(crate) fn load(dir: &Path) -> Self {
        let path = dir.join("require-locked-use-auth");
        let required =
            !matches!(fs::symlink_metadata(&path), Err(e) if e.kind() == io::ErrorKind::NotFound);
        Self { path, required }
    }

    pub(crate) fn required(&self) -> bool {
        self.required
    }

    pub(crate) fn set(&mut self, required: bool) -> io::Result<()> {
        if required {
            fs::create_dir_all(self.path.parent().expect("preference directory"))?;
            match fs::File::create_new(&self.path) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                result => {
                    result?;
                }
            }
        } else {
            match fs::remove_file(&self.path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                result => result?,
            }
        }
        self.required = required;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_preference_requires_authentication_even_after_a_failed_change() {
        let dir = tempfile::tempdir().unwrap();
        let invalid_parent = dir.path().join("not-a-directory");
        fs::write(&invalid_parent, []).unwrap();
        let mut preference = Preference::load(&invalid_parent);
        assert!(preference.required());
        assert!(preference.set(false).is_err());
        assert!(preference.required());
    }
}
