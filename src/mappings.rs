use mirajazz::{
    device::DeviceQuery,
    types::{HidDeviceInfo, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};

// Must be unique between all the plugins, 2 characters long and match `DeviceNamespace` field in `manifest.json`
pub const DEVICE_NAMESPACE: &str = "n1";

#[derive(Debug, Clone)]
pub enum Kind {
    VsdInsideN1,
}

pub const VSDINSIDE_VID: u16 = 0x5548;
pub const N1_PID: u16 = 0x1002;

// Map all queries to usage page 65440 and usage id 1 for now
pub const N1_QUERY: DeviceQuery = DeviceQuery::new(65440, 1, VSDINSIDE_VID, N1_PID);

pub const QUERIES: &[DeviceQuery] = &[N1_QUERY];

impl Kind {
    /// Matches devices VID+PID pairs to correct kinds
    pub fn from_vid_pid(vid: u16, pid: u16) -> Option<Self> {
        if vid == VSDINSIDE_VID && pid == N1_PID {
            Some(Kind::VsdInsideN1)
        } else {
            None
        }
    }

    /// There is no point relying on manufacturer/device names reported by the USB stack,
    /// so we return custom names for all the kinds of devices
    pub fn human_name(&self) -> String {
        "VSD Inside N1".to_string()
    }

    /// Returns protocol version for device
    pub fn protocol_version(&self) -> usize {
        3
    }

    pub fn row_count(&self) -> usize {
        7
    }

    pub fn col_count(&self) -> usize {
        3
    }

    pub fn key_count(&self) -> usize {
        17
    }

    pub fn encoder_count(&self) -> usize {
        1
    }

    pub fn device_type(&self) -> u8 {
        7 // StreamDeckPlus
    }

    pub fn image_format(&self) -> ImageFormat {
        ImageFormat {
            mode: ImageMode::JPEG,
            size: (96, 96),
            rotation: ImageRotation::Rot0,
            mirror: ImageMirroring::None,
        }
    }
    pub fn touch_image_format(&self) -> ImageFormat {
        ImageFormat {
            mode: ImageMode::JPEG,
            // N1 second screen segments are 64x64 each.
            size: (64, 64),
            rotation: ImageRotation::Rot0,
            mirror: ImageMirroring::None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CandidateDevice {
    pub id: String,
    pub dev: HidDeviceInfo,
    pub kind: Kind,
}
