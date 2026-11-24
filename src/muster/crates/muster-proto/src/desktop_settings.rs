//! Wire profile for the native Apple desktop service.
use crate::{Message, v1::DesktopStreamSettings};
use std::time::Duration;

impl DesktopStreamSettings {
    pub const ORIGINAL: Self = Self {
        long_edge: 1280,
        fps: 10,
        bitrate_kbps: 0,
        jpeg_quality: 65,
        show_cursor: true,
    };

    pub fn valid(&self) -> bool {
        matches!(self.long_edge, 640 | 1280 | 1920 | 2560)
            && matches!(self.fps, 5 | 10 | 15 | 30)
            && matches!(
                self.bitrate_kbps,
                0 | 1000 | 3000 | 5000 | 10000 | 20000 | 50000
            )
            && matches!(self.jpeg_quality, 45 | 65 | 85)
    }
    pub fn hello(&self) -> Vec<u8> {
        if *self == Self::ORIGINAL {
            return b"MDJ1".to_vec();
        }
        let mut hello = b"MDJ2".to_vec();
        self.encode(&mut hello)
            .expect("Vec is an unbounded protobuf writer");
        hello
    }
    pub fn from_hello(bytes: &[u8]) -> Option<Self> {
        if bytes == b"MDJ1" {
            return Some(Self::ORIGINAL);
        }
        let data = bytes.strip_prefix(b"MDJ2")?;
        if data.len() > 64 {
            return None;
        }
        Self::decode(data).ok().filter(Self::valid)
    }
    /// Minimum distance between frame starts, including the payload cost of the previous frame.
    pub fn frame_interval(&self, bytes: usize) -> Duration {
        let cadence = Duration::from_secs_f64(1.0 / f64::from(self.fps));
        if self.bitrate_kbps == 0 {
            return cadence;
        }
        cadence.max(Duration::from_secs_f64(
            bytes as f64 * 8.0 / (1000.0 * f64::from(self.bitrate_kbps)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negotiates_valid_profiles_and_paces_actual_payload() {
        let original = DesktopStreamSettings::ORIGINAL;
        assert_eq!(original.hello(), b"MDJ1");
        assert_eq!(DesktopStreamSettings::from_hello(b"MDJ1"), Some(original));
        let mut custom = DesktopStreamSettings {
            long_edge: 1920,
            bitrate_kbps: 1000,
            ..original
        };
        assert_eq!(
            DesktopStreamSettings::from_hello(&custom.hello()),
            Some(custom)
        );
        assert_eq!(custom.frame_interval(125000), Duration::from_secs(1));
        assert_eq!(custom.frame_interval(1), Duration::from_millis(100));
        custom.fps = 0;
        assert!(DesktopStreamSettings::from_hello(&custom.hello()).is_none());
        assert!(DesktopStreamSettings::from_hello(b"MDJ2").is_none());
    }
}
