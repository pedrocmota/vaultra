use base64::Engine;
use std::collections::HashMap;

const ICON_SIZE: usize = 32;

pub struct IconCache {
  entries: parking_lot::Mutex<HashMap<String, Option<String>>>,
}

impl Default for IconCache {
  fn default() -> Self {
    Self {
      entries: parking_lot::Mutex::new(HashMap::new()),
    }
  }
}

impl IconCache {
  pub fn resolve(&self, keys: &[String]) -> HashMap<String, Option<String>> {
    let mut out = HashMap::with_capacity(keys.len());
    for key in keys {
      let cached = self.entries.lock().get(key).cloned();
      let value = match cached {
        Some(value) => value,
        None => {
          let value = shell_icon_png(key).map(|png| {
            format!(
              "data:image/png;base64,{}",
              base64::engine::general_purpose::STANDARD.encode(png)
            )
          });
          self.entries.lock().insert(key.clone(), value.clone());
          value
        }
      };
      out.insert(key.clone(), value);
    }
    out
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconKind {
  Folder,
  Drive,
  Extension,
}

fn classify(key: &str) -> (IconKind, String) {
  if key == "folder" {
    return (IconKind::Folder, "folder".into());
  }
  if let Some(drive) = key.strip_prefix("drive:") {
    return (IconKind::Drive, drive.to_string());
  }
  let extension = key.strip_prefix("ext:").unwrap_or(key);
  (
    IconKind::Extension,
    if extension.is_empty() {
      "file".into()
    } else {
      format!(".{extension}")
    },
  )
}

fn encode_png(rgba: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
  let mut buffer = Vec::new();
  {
    let mut encoder = png::Encoder::new(&mut buffer, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
  }
  Some(buffer)
}

#[cfg(windows)]
fn shell_icon_png(key: &str) -> Option<Vec<u8>> {
  win::shell_icon_rgba(key).and_then(|rgba| encode_png(&rgba, ICON_SIZE, ICON_SIZE))
}

#[cfg(not(windows))]
fn shell_icon_png(_key: &str) -> Option<Vec<u8>> {
  None
}

#[cfg(windows)]
mod win {
  use super::{classify, IconKind, ICON_SIZE};
  use windows::core::PCWSTR;
  use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
  };
  use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
  };
  use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
  };
  use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

  fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
  }

  pub fn shell_icon_rgba(key: &str) -> Option<Vec<u8>> {
    let (kind, path) = classify(key);
    let (attributes, flags) = match kind {
      IconKind::Folder => (
        FILE_ATTRIBUTE_DIRECTORY,
        SHGFI_ICON | SHGFI_LARGEICON | SHGFI_USEFILEATTRIBUTES,
      ),
      IconKind::Drive => (FILE_FLAGS_AND_ATTRIBUTES(0), SHGFI_ICON | SHGFI_LARGEICON),
      IconKind::Extension => (
        FILE_ATTRIBUTE_NORMAL,
        SHGFI_ICON | SHGFI_LARGEICON | SHGFI_USEFILEATTRIBUTES,
      ),
    };
    let path = wide(&path);
    let mut info = SHFILEINFOW::default();
    let result = unsafe {
      SHGetFileInfoW(
        PCWSTR(path.as_ptr()),
        attributes,
        Some(&mut info),
        std::mem::size_of::<SHFILEINFOW>() as u32,
        flags,
      )
    };
    if result == 0 || info.hIcon.is_invalid() {
      return None;
    }
    let pixels = icon_to_rgba(info.hIcon);
    unsafe {
      let _ = DestroyIcon(info.hIcon);
    }
    pixels
  }

  fn icon_to_rgba(icon: HICON) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    let color = read_bitmap(info.hbmColor, 32);
    let mask = read_bitmap(info.hbmMask, 32);
    unsafe {
      let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
      let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
    }
    let mut bgra = color?;
    let has_alpha = bgra.chunks_exact(4).any(|px| px[3] != 0);
    if !has_alpha {
      if let Some(mask) = mask {
        for (px, m) in bgra.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
          px[3] = if m[0] == 0 { 255 } else { 0 };
        }
      } else {
        for px in bgra.chunks_exact_mut(4) {
          px[3] = 255;
        }
      }
    }
    for px in bgra.chunks_exact_mut(4) {
      px.swap(0, 2);
    }
    Some(bgra)
  }

  fn read_bitmap(bitmap: windows::Win32::Graphics::Gdi::HBITMAP, bits: u16) -> Option<Vec<u8>> {
    if bitmap.is_invalid() {
      return None;
    }
    let mut header = BITMAPINFO {
      bmiHeader: BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: ICON_SIZE as i32,
        biHeight: -(ICON_SIZE as i32),
        biPlanes: 1,
        biBitCount: bits,
        biCompression: BI_RGB.0,
        ..Default::default()
      },
      ..Default::default()
    };
    let mut buffer = vec![0u8; ICON_SIZE * ICON_SIZE * 4];
    let dc = unsafe { GetDC(None) };
    let lines = unsafe {
      GetDIBits(
        dc,
        bitmap,
        0,
        ICON_SIZE as u32,
        Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
        &mut header,
        DIB_RGB_COLORS,
      )
    };
    unsafe {
      ReleaseDC(None, dc);
    }
    (lines > 0).then_some(buffer)
  }
}

#[cfg(test)]
mod tests {
  use super::IconCache;

  #[test]
  fn resolves_shell_icons() {
    let cache = IconCache::default();
    let keys: Vec<String> = [
      "folder",
      "drive:C:\\",
      "ext:txt",
      "ext:js",
      "ext:exe",
      "ext:",
    ]
    .iter()
    .map(|k| k.to_string())
    .collect();
    let icons = cache.resolve(&keys);
    for key in &keys {
      let icon = icons.get(key).cloned().flatten();
      assert!(icon.is_some(), "no icon for {key}");
    }
  }
}
