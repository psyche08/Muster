//! Ghostty as a terminal source: muster-terminal's interface over the macOS
//! adapter (AppleScript and accessibility).
use muster_platform::macos::ghostty as os;
use muster_terminal::Error;
use muster_terminal::ghostty::{GhosttyOs, Listed, Surface};

pub struct Ghostty;

fn unavailable(message: String) -> Error {
    Error::Unavailable(message)
}

struct Screen(os::Surface);

impl Surface for Screen {
    fn text(&mut self) -> Result<Option<String>, Error> {
        self.0.text().map_err(unavailable)
    }
}

impl GhosttyOs for Ghostty {
    fn running(&self) -> bool {
        os::running()
    }
    fn terminals(&self) -> Result<Vec<Listed>, Error> {
        Ok(os::terminals()
            .map_err(unavailable)?
            .into_iter()
            .map(|(id, window, tab, title, cwd)| Listed {
                id,
                title,
                cwd,
                window,
                tab,
            })
            .collect())
    }
    fn perform(&self, id: &str, action: &str) -> Result<(), Error> {
        os::perform(id, action).map_err(unavailable)
    }
    fn open(&self, id: &str) -> Result<Box<dyn Surface>, Error> {
        Ok(Box::new(Screen(os::open(id).map_err(unavailable)?)))
    }
    fn create(&self) -> Result<String, Error> {
        os::create().map_err(unavailable)
    }
    fn close(&self, id: &str) -> Result<(), Error> {
        os::close(id).map_err(unavailable)
    }
}

/// `--terminal auto` on a Mac: iTerm2 when it is installed, else Ghostty when
/// it is, else iTerm2 (whose setup then says it is missing).
pub fn auto() -> muster_terminal::TerminalKind {
    let iterm = muster_platform::macos::automation::installed("com.googlecode.iterm2");
    if iterm != Some(true) && os::installed() {
        return ghostty();
    }
    muster_terminal::TerminalKind::Iterm2(muster_terminal::TerminalConfig::default())
}

pub fn ghostty() -> muster_terminal::TerminalKind {
    muster_terminal::TerminalKind::Ghostty(muster_terminal::GhosttyPlatform(std::sync::Arc::new(
        Ghostty,
    )))
}
