use crate::error::VResult;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFiles {
  pub sequence: u32,
  pub paths: Vec<String>,
  pub cut: bool,
  pub token: Option<String>,
}

pub fn write(text: &str, files: &[String], token: &str) -> VResult<u32> {
  platform::write(text, files, token)
}

pub fn read_files() -> VResult<ClipboardFiles> {
  platform::read_files()
}

#[cfg(windows)]
mod platform {
  use super::ClipboardFiles;
  use crate::drag_out::hglobal;
  use crate::error::{VError, VResult};
  use std::time::Duration;
  use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
  use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
  };
  use windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_MOVE};
  use windows::Win32::UI::Shell::{DragQueryFileW, CFSTR_PREFERREDDROPEFFECT, HDROP};

  const OPEN_ATTEMPTS: usize = 10;
  const OPEN_RETRY_DELAY: Duration = Duration::from_millis(30);

  struct OpenedClipboard;

  impl OpenedClipboard {
    fn open() -> VResult<Self> {
      for attempt in 0..OPEN_ATTEMPTS {
        if unsafe { OpenClipboard(None) }.is_ok() {
          return Ok(Self);
        }
        if attempt + 1 < OPEN_ATTEMPTS {
          std::thread::sleep(OPEN_RETRY_DELAY);
        }
      }
      Err(VError::Other(
        "the clipboard is in use by another application".into(),
      ))
    }
  }

  impl Drop for OpenedClipboard {
    fn drop(&mut self) {
      unsafe {
        let _ = CloseClipboard();
      }
    }
  }

  fn preferred_effect_format() -> u32 {
    unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) }
  }

  fn token_format() -> u32 {
    unsafe { RegisterClipboardFormatW(windows::core::w!("Vaultra.ClipboardToken")) }
  }

  unsafe fn put(format: u32, global: HGLOBAL) -> VResult<()> {
    if let Err(error) = SetClipboardData(format, Some(HANDLE(global.0))) {
      let _ = GlobalFree(Some(global));
      return Err(VError::Other(format!(
        "cannot write to the clipboard: {error}"
      )));
    }
    Ok(())
  }

  pub fn write(text: &str, files: &[String], token: &str) -> VResult<u32> {
    {
      let _clipboard = OpenedClipboard::open()?;
      unsafe {
        EmptyClipboard().map_err(|e| VError::Other(format!("cannot clear the clipboard: {e}")))?;
        put(CF_UNICODETEXT.0 as u32, hglobal::text(text)?)?;
        put(token_format(), hglobal::text(token)?)?;
        if !files.is_empty() {
          put(CF_HDROP.0 as u32, hglobal::drop_files(files)?)?;
          put(
            preferred_effect_format(),
            hglobal::dword(DROPEFFECT_COPY.0)?,
          )?;
        }
      }
    }
    Ok(unsafe { GetClipboardSequenceNumber() })
  }

  pub fn read_files() -> VResult<ClipboardFiles> {
    let _clipboard = OpenedClipboard::open()?;
    unsafe {
      let sequence = GetClipboardSequenceNumber();
      let token = GetClipboardData(token_format())
        .ok()
        .and_then(|handle| hglobal::read_text(HGLOBAL(handle.0)));
      if IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_err() {
        return Ok(ClipboardFiles {
          sequence,
          token,
          ..Default::default()
        });
      }
      let handle = GetClipboardData(CF_HDROP.0 as u32)
        .map_err(|e| VError::Other(format!("cannot read the clipboard: {e}")))?;
      let drop = HDROP(handle.0);
      let count = DragQueryFileW(drop, u32::MAX, None);
      let mut paths = Vec::with_capacity(count as usize);
      for index in 0..count {
        let length = DragQueryFileW(drop, index, None) as usize;
        let mut buffer = vec![0u16; length + 1];
        DragQueryFileW(drop, index, Some(&mut buffer));
        paths.push(String::from_utf16_lossy(&buffer[..length]));
      }
      let cut = GetClipboardData(preferred_effect_format())
        .ok()
        .and_then(|handle| hglobal::read_dword(HGLOBAL(handle.0)))
        .map(|effect| effect & DROPEFFECT_MOVE.0 != 0)
        .unwrap_or(false);
      Ok(ClipboardFiles {
        sequence,
        paths,
        cut,
        token,
      })
    }
  }
}

#[cfg(not(windows))]
mod platform {
  use super::ClipboardFiles;
  use crate::error::{VError, VResult};

  pub fn write(_text: &str, _files: &[String], _token: &str) -> VResult<u32> {
    Err(VError::Unsupported(
      "the system clipboard is only available on Windows".into(),
    ))
  }

  pub fn read_files() -> VResult<ClipboardFiles> {
    Ok(ClipboardFiles::default())
  }
}
