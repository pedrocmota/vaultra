use crate::error::{VError, VResult};

const TARGET_PREFIX: &str = "Vaultra/";

pub trait CredentialStore: Send + Sync {
  fn get(&self, site_id: &str) -> VResult<Option<String>>;
  fn set(&self, site_id: &str, user: &str, secret: &str) -> VResult<()>;
  fn delete(&self, site_id: &str) -> VResult<()>;
}

fn target_name(site_id: &str) -> String {
  format!("{TARGET_PREFIX}{site_id}")
}

#[cfg(windows)]
pub use win::WindowsCredentialStore as PlatformCredentialStore;

#[cfg(windows)]
mod win {
  use super::*;
  use windows::core::{PCWSTR, PWSTR};
  use windows::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME};
  use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
  };

  #[derive(Default)]
  pub struct WindowsCredentialStore;

  fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
  }

  impl CredentialStore for WindowsCredentialStore {
    fn get(&self, site_id: &str) -> VResult<Option<String>> {
      let target = wide(&target_name(site_id));
      let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
      let result = unsafe {
        CredReadW(
          PCWSTR(target.as_ptr()),
          CRED_TYPE_GENERIC,
          None,
          &mut credential,
        )
      };
      match result {
        Ok(()) => {
          let secret = unsafe {
            let cred = &*credential;
            let bytes =
              std::slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize);
            let utf16: Vec<u16> = bytes
              .chunks_exact(2)
              .map(|c| u16::from_le_bytes([c[0], c[1]]))
              .collect();
            let secret = String::from_utf16_lossy(&utf16);
            CredFree(credential as *const std::ffi::c_void);
            secret
          };
          Ok(Some(secret))
        }
        Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => Ok(None),
        Err(e) => Err(VError::Other(format!("credential read failed: {e}"))),
      }
    }

    fn set(&self, site_id: &str, user: &str, secret: &str) -> VResult<()> {
      let mut target = wide(&target_name(site_id));
      let mut user_name = wide(user);
      let mut blob: Vec<u8> = secret
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
      let mut comment = wide("Vaultra site password");
      let credential = CREDENTIALW {
        Flags: Default::default(),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        Comment: PWSTR(comment.as_mut_ptr()),
        LastWritten: FILETIME::default(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: PWSTR::null(),
        UserName: PWSTR(user_name.as_mut_ptr()),
      };
      unsafe { CredWriteW(&credential, 0) }
        .map_err(|e| VError::Other(format!("credential write failed: {e}")))
    }

    fn delete(&self, site_id: &str) -> VResult<()> {
      let target = wide(&target_name(site_id));
      match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => Ok(()),
        Err(e) => Err(VError::Other(format!("credential delete failed: {e}"))),
      }
    }
  }
}

#[cfg(not(windows))]
pub use fallback::MemoryCredentialStore as PlatformCredentialStore;

#[cfg(not(windows))]
mod fallback {
  use super::*;
  use std::collections::HashMap;

  #[derive(Default)]
  pub struct MemoryCredentialStore {
    map: parking_lot::Mutex<HashMap<String, String>>,
  }

  impl CredentialStore for MemoryCredentialStore {
    fn get(&self, site_id: &str) -> VResult<Option<String>> {
      Ok(self.map.lock().get(site_id).cloned())
    }
    fn set(&self, site_id: &str, _user: &str, secret: &str) -> VResult<()> {
      self
        .map
        .lock()
        .insert(site_id.to_string(), secret.to_string());
      Ok(())
    }
    fn delete(&self, site_id: &str) -> VResult<()> {
      self.map.lock().remove(site_id);
      Ok(())
    }
  }
}
