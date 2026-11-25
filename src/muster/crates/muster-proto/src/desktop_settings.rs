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
            && matches!(self.fps, 5 | 10 | 15 | 30 | 60)
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
        Self::decode(data).ok().filter(|s| s.valid() && s.fps <= 30)
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
    fn keyboard_hello_preserves_video_settings_for_pre_keyboard_hosts() {
        let settings = DesktopStreamSettings::ORIGINAL;
        let hello = settings.keyboard_video_hello();
        assert_eq!(
            DesktopStreamSettings::from_video_hello(&hello),
            Some(settings)
        );
        assert!(DesktopStreamSettings::viewer_has_keyboard(&hello));
        assert!(!DesktopStreamSettings::viewer_has_keyboard(
            &settings.video_hello()
        ));
        assert!(!DesktopStreamSettings::viewer_has_keyboard(b"MDJ1"));
    }
    #[test]
    fn video_negotiates_even_pixel_extents_and_encoder_ceiling() {
        let settings = DesktopStreamSettings {
            long_edge: 1920,
            bitrate_kbps: 5000,
            fps: 30,
            ..DesktopStreamSettings::ORIGINAL
        };
        assert_eq!(
            DesktopStreamSettings::from_video_hello(&settings.video_hello()),
            Some(settings)
        );
        assert_eq!(settings.video_size(3456, 2234), Some((1920, 1240)));
        assert_eq!(settings.video_size(641, 481), Some((640, 480)));
        assert_eq!(settings.video_size(0, 480), None);
        assert_eq!(settings.video_kbps(1920, 1240), 5000);
        assert!(DesktopStreamSettings::from_video_hello(b"MDV1").is_none());
    }
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

impl DesktopStreamSettings {
    pub fn video_hello(&self) -> Vec<u8> {
        let mut bytes = b"MDV2".to_vec();
        bytes.extend(self.encode_to_vec());
        bytes
    }
    // Protobuf field 16 in the MDV2 hello is ignored by pre-keyboard hosts.
    pub fn keyboard_video_hello(&self) -> Vec<u8> {
        let mut hello = self.video_hello();
        KeyboardHello { keyboard: true }
            .encode(&mut hello)
            .expect("Vec writer");
        hello
    }
    pub fn viewer_has_keyboard(bytes: &[u8]) -> bool {
        bytes
            .strip_prefix(b"MDV2")
            .filter(|data| data.len() <= 64)
            .and_then(|data| KeyboardHello::decode(data).ok())
            .is_some_and(|hello| hello.keyboard)
    }
    pub fn from_video_hello(bytes: &[u8]) -> Option<Self> {
        let data = bytes.strip_prefix(b"MDV2")?;
        (data.len() <= 64)
            .then(|| Self::decode(data).ok())
            .flatten()
            .filter(Self::valid)
    }
    pub fn video_size(&self, width: u32, height: u32) -> Option<(u16, u16)> {
        if width < 2 || height < 2 || !self.valid() {
            return None;
        }
        let longest = u64::from(width.max(height));
        let scale = longest.min(u64::from(self.long_edge));
        let even = |n| ((u64::from(n) * scale / longest).max(2) as u16) & !1;
        Some((even(width), even(height)))
    }
    pub fn video_kbps(&self, width: u16, height: u16) -> u32 {
        let pixels_per_second = u64::from(width) * u64::from(height) * u64::from(self.fps);
        let automatic =
            (pixels_per_second * u64::from(self.jpeg_quality) / 450000).clamp(500, 50000) as u32;
        if self.bitrate_kbps == 0 {
            automatic
        } else {
            automatic.min(self.bitrate_kbps)
        }
    }
}

#[derive(Clone, PartialEq, Message)]
struct KeyboardHello {
    #[prost(bool, tag = "16")]
    keyboard: bool,
}
