use muster_proto::{Message, v1::DesktopStreamSettings};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(crate) struct Preferences {
    dir: PathBuf,
    pub settings: DesktopStreamSettings,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            dir: PathBuf::new(),
            settings: DesktopStreamSettings::ORIGINAL,
        }
    }
}
impl Preferences {
    pub fn load(dir: &Path) -> Self {
        let settings = std::fs::read(dir.join("desktop-stream.pb"))
            .ok()
            .and_then(|b| DesktopStreamSettings::decode(b.as_slice()).ok())
            .filter(DesktopStreamSettings::valid)
            .unwrap_or(DesktopStreamSettings::ORIGINAL);
        Self {
            dir: dir.into(),
            settings,
        }
    }
    pub fn save(&mut self, settings: DesktopStreamSettings) -> io::Result<()> {
        if !settings.valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unsupported desktop stream settings",
            ));
        }
        std::fs::create_dir_all(&self.dir)?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.dir)?;
        temp.write_all(&settings.encode_to_vec())?;
        temp.persist(self.dir.join("desktop-stream.pb"))?;
        self.settings = settings;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_the_last_saved_valid_profile() {
        let dir = tempfile::tempdir().unwrap();
        let mut pref = Preferences::load(dir.path());
        let custom = DesktopStreamSettings {
            fps: 15,
            ..pref.settings
        };
        pref.save(custom).unwrap();
        assert!(
            pref.save(DesktopStreamSettings {
                long_edge: u32::MAX,
                ..custom
            })
            .is_err()
        );
        assert_eq!(Preferences::load(dir.path()).settings, custom);
    }
}
