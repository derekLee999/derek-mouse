use std::{
    ffi::c_void,
    fs::{self, File},
    mem::size_of,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use image::{codecs::jpeg::JpegEncoder, ExtendedColorType};
use serde::Serialize;
use windows::Win32::{
    Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    },
    UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    },
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSnapshot {
    pub image_path: PathBuf,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

pub fn capture_virtual_screen() -> Result<ScreenSnapshot, String> {
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    if width <= 0 || height <= 0 {
        return Err("Could not determine screen size".to_string());
    }

    let bgra = capture_bgra_pixels(left, top, width, height)?;
    let image_path = snapshot_path()?;
    write_jpeg(&image_path, width, height, &bgra)?;

    Ok(ScreenSnapshot {
        image_path,
        left,
        top,
        width: width as u32,
        height: height as u32,
    })
}

fn capture_bgra_pixels(left: i32, top: i32, width: i32, height: i32) -> Result<Vec<u8>, String> {
    unsafe {
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err("Could not access screen device context".to_string());
        }

        let result = (|| {
            let memory_dc = CreateCompatibleDC(Some(screen_dc));
            if memory_dc.is_invalid() {
                return Err("Could not create memory device context".to_string());
            }

            let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
            if bitmap.is_invalid() {
                let _ = DeleteDC(memory_dc);
                return Err("Could not create screen bitmap".to_string());
            }

            let old_object = SelectObject(memory_dc, HGDIOBJ(bitmap.0));
            let copied = BitBlt(
                memory_dc,
                0,
                0,
                width,
                height,
                Some(screen_dc),
                left,
                top,
                SRCCOPY,
            );
            if copied.is_err() {
                let _ = SelectObject(memory_dc, old_object);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(memory_dc);
                return Err("Could not copy screen bitmap".to_string());
            }

            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut pixels = vec![0_u8; width as usize * height as usize * 4];
            let lines = GetDIBits(
                memory_dc,
                bitmap,
                0,
                height as u32,
                Some(pixels.as_mut_ptr() as *mut c_void),
                &mut info,
                DIB_RGB_COLORS,
            );

            let _ = SelectObject(memory_dc, old_object);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory_dc);

            if lines == 0 {
                return Err("Could not read screen bitmap pixels".to_string());
            }

            Ok(pixels)
        })();

        let _ = ReleaseDC(None, screen_dc);
        result
    }
}

fn write_jpeg(path: &PathBuf, width: i32, height: i32, bgra: &[u8]) -> Result<(), String> {
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for pixel in bgra.chunks_exact(4) {
        rgb.push(pixel[2]);
        rgb.push(pixel[1]);
        rgb.push(pixel[0]);
    }

    let file = File::create(path).map_err(|err| err.to_string())?;
    JpegEncoder::new_with_quality(file, 82)
        .encode(&rgb, width as u32, height as u32, ExtendedColorType::Rgb8)
        .map_err(|err| err.to_string())
}

fn snapshot_path() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("derek-mouse");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    Ok(dir.join(format!("coordinate-picker-{timestamp}.jpg")))
}
