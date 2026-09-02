use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub type Timestamp = i64;

pub fn now() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId(pub i64);

impl std::fmt::Display for EntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum EntryKind {
    Text = 0,
    Image = 1,
    Files = 2,
}

impl EntryKind {
    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Text),
            1 => Some(Self::Image),
            2 => Some(Self::Files),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flavor {
    pub mime: String,
    pub body: Vec<u8>,
}

impl Flavor {
    pub fn new(mime: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            mime: mime.into(),
            body,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub kind: EntryKind,
    pub flavors: Vec<Flavor>,
    pub source_app: Option<String>,
}

impl Capture {
    pub fn primary(&self) -> Option<&Flavor> {
        self.flavors.first()
    }

    pub fn byte_size(&self) -> u64 {
        self.flavors
            .iter()
            .map(|flavor| flavor.body.len() as u64)
            .sum()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryMeta {
    pub id: EntryId,
    pub kind: EntryKind,
    pub preview: String,
    pub byte_size: u64,
    pub source_app: Option<String>,
    pub created_at: Timestamp,
    pub last_used_at: Timestamp,
    pub use_count: u32,
    pub pinned: Option<u32>,
    pub image_size: Option<(u32, u32)>,
}

impl EntryMeta {
    pub fn label(&self) -> &str {
        &self.preview
    }
}

pub const PREVIEW_CHARS: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Ext,
    Wlr,
}

impl Backend {
    pub fn protocol(self) -> &'static str {
        match self {
            Self::Ext => "ext_data_control_manager_v1",
            Self::Wlr => "zwlr_data_control_manager_v1",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CaptureState {
    #[default]
    Starting,
    Active(Backend),
    Paused,
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub capture: CaptureState,
    pub entries: Arc<[EntryMeta]>,
    pub revision: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_after_2020() {
        assert!(now() > 1_577_836_800_000);
    }

    #[test]
    fn entry_kind_round_trips_through_its_discriminant() {
        for kind in [EntryKind::Text, EntryKind::Image, EntryKind::Files] {
            assert_eq!(EntryKind::from_i64(kind as i64), Some(kind));
        }
        assert_eq!(EntryKind::from_i64(3), None);
        assert_eq!(EntryKind::from_i64(-1), None);
    }

    #[test]
    fn capture_size_counts_every_flavor() {
        let capture = Capture {
            kind: EntryKind::Text,
            flavors: vec![
                Flavor::new("text/plain", b"hello".to_vec()),
                Flavor::new("text/html", b"<b>hello</b>".to_vec()),
            ],
            source_app: None,
        };

        assert_eq!(capture.byte_size(), 17);
        assert_eq!(capture.primary().unwrap().mime, "text/plain");
    }
}
