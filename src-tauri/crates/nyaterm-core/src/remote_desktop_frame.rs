use crate::error::{AppError, AppResult};

pub const FRAME_HEADER_BYTES: usize = 44;
pub const PIXEL_FORMAT_BGRA8888: u32 = 1;
pub const PIXEL_FORMAT_RGBA8888: u32 = 2;
const BYTES_PER_PIXEL: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RemoteDesktopPixelFormat {
    Bgra8888,
    Rgba8888,
}

impl RemoteDesktopPixelFormat {
    fn wire_value(self) -> u32 {
        match self {
            Self::Bgra8888 => PIXEL_FORMAT_BGRA8888,
            Self::Rgba8888 => PIXEL_FORMAT_RGBA8888,
        }
    }
}

pub struct RemoteDesktopFramePatch<'a> {
    pub sequence: u64,
    pub desktop_width: u32,
    pub desktop_height: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixel_format: RemoteDesktopPixelFormat,
    pub payload: &'a [u8],
}

pub fn encode_frame_patch(patch: &RemoteDesktopFramePatch<'_>) -> AppResult<Vec<u8>> {
    validate_frame_patch(patch)?;
    let mut frame = Vec::with_capacity(
        FRAME_HEADER_BYTES
            .checked_add(patch.payload.len())
            .ok_or_else(|| AppError::Config("Remote desktop frame size overflows".to_string()))?,
    );
    frame.extend_from_slice(&patch.sequence.to_le_bytes());
    frame.extend_from_slice(&patch.desktop_width.to_le_bytes());
    frame.extend_from_slice(&patch.desktop_height.to_le_bytes());
    frame.extend_from_slice(&patch.x.to_le_bytes());
    frame.extend_from_slice(&patch.y.to_le_bytes());
    frame.extend_from_slice(&patch.width.to_le_bytes());
    frame.extend_from_slice(&patch.height.to_le_bytes());
    frame.extend_from_slice(&patch.stride.to_le_bytes());
    frame.extend_from_slice(&patch.pixel_format.wire_value().to_le_bytes());
    let payload_len = u32::try_from(patch.payload.len()).map_err(|_| {
        AppError::Config("Remote desktop frame payload exceeds the wire limit".to_string())
    })?;
    frame.extend_from_slice(&payload_len.to_le_bytes());
    frame.extend_from_slice(patch.payload);
    Ok(frame)
}

fn validate_frame_patch(patch: &RemoteDesktopFramePatch<'_>) -> AppResult<()> {
    if patch.desktop_width == 0 || patch.desktop_height == 0 {
        return Err(AppError::Config(
            "Remote desktop framebuffer dimensions must be non-zero".to_string(),
        ));
    }
    if patch.width == 0 || patch.height == 0 {
        return Err(AppError::Config(
            "Remote desktop frame rectangle must be non-zero".to_string(),
        ));
    }
    let right = patch.x.checked_add(patch.width).ok_or_else(|| {
        AppError::Config("Remote desktop frame horizontal bounds overflow".to_string())
    })?;
    let bottom = patch.y.checked_add(patch.height).ok_or_else(|| {
        AppError::Config("Remote desktop frame vertical bounds overflow".to_string())
    })?;
    if right > patch.desktop_width || bottom > patch.desktop_height {
        return Err(AppError::Config(
            "Remote desktop frame rectangle exceeds framebuffer bounds".to_string(),
        ));
    }
    let row_bytes = usize::try_from(patch.width)
        .ok()
        .and_then(|width| width.checked_mul(BYTES_PER_PIXEL))
        .ok_or_else(|| AppError::Config("Remote desktop frame row size overflows".to_string()))?;
    let stride = usize::try_from(patch.stride)
        .map_err(|_| AppError::Config("Remote desktop frame stride is invalid".to_string()))?;
    if stride < row_bytes {
        return Err(AppError::Config(
            "Remote desktop frame stride is too small".to_string(),
        ));
    }
    let required_payload = usize::try_from(patch.height)
        .ok()
        .and_then(|height| stride.checked_mul(height))
        .ok_or_else(|| {
            AppError::Config("Remote desktop frame payload size overflows".to_string())
        })?;
    if patch.payload.len() < required_payload {
        return Err(AppError::Config(
            "Remote desktop frame payload is too small".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_shared_frame_header() {
        let payload = [1, 2, 3, 255];
        let frame = encode_frame_patch(&RemoteDesktopFramePatch {
            sequence: 7,
            desktop_width: 10,
            desktop_height: 10,
            x: 2,
            y: 3,
            width: 1,
            height: 1,
            stride: 4,
            pixel_format: RemoteDesktopPixelFormat::Rgba8888,
            payload: &payload,
        })
        .expect("frame should encode");

        assert_eq!(frame.len(), FRAME_HEADER_BYTES + payload.len());
        assert_eq!(u64::from_le_bytes(frame[0..8].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(frame[36..40].try_into().unwrap()), 2);
        assert_eq!(&frame[FRAME_HEADER_BYTES..], &payload);
    }

    #[test]
    fn rejects_rectangles_outside_the_framebuffer() {
        let error = encode_frame_patch(&RemoteDesktopFramePatch {
            sequence: 1,
            desktop_width: 10,
            desktop_height: 10,
            x: 10,
            y: 0,
            width: 1,
            height: 1,
            stride: 4,
            pixel_format: RemoteDesktopPixelFormat::Rgba8888,
            payload: &[0; 4],
        })
        .expect_err("out-of-bounds rectangle must fail");

        assert!(error.to_string().contains("exceeds framebuffer bounds"));
    }
}
