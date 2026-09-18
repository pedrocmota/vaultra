use crate::error::{VError, VResult};
use crate::transfer::{QueueRequest, TransferQueue};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DragOutcome {
  Dropped,
  Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropEffect {
  None,
  Copy,
  Move,
  Link,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragOutResult {
  pub outcome: DragOutcome,
  pub effect: DropEffect,
}

pub struct DeferredDownload {
  pub queue: Arc<TransferQueue>,
  pub batch: String,
  pub requests: Vec<QueueRequest>,
}

pub struct DragJob {
  pub owner_window: isize,
  pub paths: Vec<PathBuf>,
  pub allow_move: bool,
  pub allow_link: bool,
  pub prefer_move: bool,
  pub deferred: Option<DeferredDownload>,
}

pub fn install(app: &AppHandle) {
  platform::install(app);
}

pub async fn start(job: DragJob) -> VResult<DragOutResult> {
  let (tx, rx) = oneshot::channel();
  platform::post(job, tx)?;
  rx.await.map_err(|_| VError::Cancelled)?
}

#[cfg(windows)]
mod platform {
  use super::{DeferredDownload, DragJob, DragOutResult, DragOutcome, DropEffect};
  use crate::error::{VError, VResult};
  use std::cell::Cell;
  use std::sync::atomic::{AtomicIsize, Ordering};
  use std::sync::Arc;
  use std::time::Duration;
  use tauri::AppHandle;
  use tokio::sync::oneshot;
  use windows::core::{implement, w, BOOL, HRESULT};
  use windows::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HGLOBAL, HWND, LPARAM,
    LRESULT, POINT, S_OK, WPARAM,
  };
  use windows::Win32::System::Com::{
    IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
  };
  use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
  use windows::Win32::System::LibraryLoader::GetModuleHandleW;
  use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY,
    DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
  };
  use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
  use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
  use windows::Win32::UI::Shell::{SHCreateDataObject, CFSTR_PREFERREDDROPEFFECT};
  use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetAncestor, GetCursorPos,
    MsgWaitForMultipleObjectsEx, PeekMessageW, PostMessageW, RegisterClassW, TranslateMessage,
    WindowFromPoint, GA_ROOT, HWND_MESSAGE, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WNDCLASSW,
  };

  const WM_START_DRAG: u32 = WM_APP + 0x51;
  const POLL_INTERVAL: Duration = Duration::from_millis(50);

  static HOST_WINDOW: AtomicIsize = AtomicIsize::new(0);

  struct Posted {
    job: DragJob,
    done: oneshot::Sender<VResult<DragOutResult>>,
  }

  pub fn install(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| unsafe {
      match create_host_window() {
        Ok(hwnd) => {
          HOST_WINDOW.store(hwnd.0 as isize, Ordering::Release);
          tracing::info!("native drag host ready");
        }
        Err(error) => tracing::error!("drag host window unavailable: {error}"),
      }
    });
  }

  pub fn post(job: DragJob, done: oneshot::Sender<VResult<DragOutResult>>) -> VResult<()> {
    let hwnd = HOST_WINDOW.load(Ordering::Acquire);
    if hwnd == 0 {
      return Err(VError::Unsupported("native drag host is not ready".into()));
    }
    let payload = Box::into_raw(Box::new(Posted { job, done }));
    let posted = unsafe {
      PostMessageW(
        Some(HWND(hwnd as _)),
        WM_START_DRAG,
        WPARAM(0),
        LPARAM(payload as isize),
      )
    };
    if let Err(error) = posted {
      drop(unsafe { Box::from_raw(payload) });
      return Err(VError::Other(format!("cannot start native drag: {error}")));
    }
    Ok(())
  }

  unsafe fn create_host_window() -> windows::core::Result<HWND> {
    let class_name = w!("VaultraDragHost");
    let instance = GetModuleHandleW(None)?;
    let class = WNDCLASSW {
      lpfnWndProc: Some(host_proc),
      hInstance: instance.into(),
      lpszClassName: class_name,
      ..Default::default()
    };
    RegisterClassW(&class);
    CreateWindowExW(
      WINDOW_EX_STYLE(0),
      class_name,
      w!("Vaultra drag host"),
      WINDOW_STYLE(0),
      0,
      0,
      0,
      0,
      Some(HWND_MESSAGE),
      None,
      Some(instance.into()),
      None,
    )
  }

  unsafe extern "system" fn host_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
  ) -> LRESULT {
    if message == WM_START_DRAG {
      let posted = Box::from_raw(lparam.0 as *mut Posted);
      let result = perform(posted.job);
      let _ = posted.done.send(result);
      return LRESULT(0);
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
  }

  unsafe fn perform(job: DragJob) -> VResult<DragOutResult> {
    let data: IDataObject = SHCreateDataObject(None, None, None)
      .map_err(|e| VError::Other(format!("cannot create drag data object: {e}")))?;
    let paths: Vec<String> = job
      .paths
      .iter()
      .map(|p| p.to_string_lossy().into_owned())
      .collect();
    set_global(&data, CF_HDROP.0, super::hglobal::drop_files(&paths)?)?;
    if job.prefer_move {
      let format = RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) as u16;
      set_global(&data, format, super::hglobal::dword(DROPEFFECT_MOVE.0)?)?;
    }
    let failure = Arc::new(parking_lot::Mutex::new(None));
    let source: IDropSource = DropSource {
      owner_window: job.owner_window,
      deferred: job.deferred,
      rendered: Cell::new(false),
      last_effect: Cell::new(DROPEFFECT_NONE),
      failure: failure.clone(),
    }
    .into();
    let mut allowed = DROPEFFECT_COPY;
    if job.allow_move {
      allowed |= DROPEFFECT_MOVE;
    }
    if job.allow_link {
      allowed |= DROPEFFECT_LINK;
    }
    let mut effect = DROPEFFECT_NONE;
    let status = DoDragDrop(&data, &source, allowed, &mut effect);
    if let Some(error) = failure.lock().take() {
      return Err(error);
    }
    if status == DRAGDROP_S_DROP {
      return Ok(DragOutResult {
        outcome: DragOutcome::Dropped,
        effect: effect_of(effect),
      });
    }
    if status == DRAGDROP_S_CANCEL {
      return Ok(DragOutResult {
        outcome: DragOutcome::Cancelled,
        effect: DropEffect::None,
      });
    }
    Err(VError::Other(format!("native drag failed: {status:?}")))
  }

  fn effect_of(effect: DROPEFFECT) -> DropEffect {
    if effect.0 & DROPEFFECT_MOVE.0 != 0 {
      DropEffect::Move
    } else if effect.0 & DROPEFFECT_COPY.0 != 0 {
      DropEffect::Copy
    } else if effect.0 & DROPEFFECT_LINK.0 != 0 {
      DropEffect::Link
    } else {
      DropEffect::None
    }
  }

  unsafe fn set_global(data: &IDataObject, format: u16, global: HGLOBAL) -> VResult<()> {
    let format_etc = FORMATETC {
      cfFormat: format,
      ptd: std::ptr::null_mut(),
      dwAspect: DVASPECT_CONTENT.0,
      lindex: -1,
      tymed: TYMED_HGLOBAL.0 as u32,
    };
    let medium = STGMEDIUM {
      tymed: TYMED_HGLOBAL.0 as u32,
      u: STGMEDIUM_0 { hGlobal: global },
      pUnkForRelease: std::mem::ManuallyDrop::new(None),
    };
    data
      .SetData(&format_etc, &medium, true)
      .map_err(|e| VError::Other(format!("cannot fill drag data object: {e}")))
  }

  #[implement(IDropSource)]
  struct DropSource {
    owner_window: isize,
    deferred: Option<DeferredDownload>,
    rendered: Cell<bool>,
    last_effect: Cell<DROPEFFECT>,
    failure: Arc<parking_lot::Mutex<Option<VError>>>,
  }

  impl DropSource_Impl {
    fn render_before_drop(&self) -> HRESULT {
      if self.rendered.replace(true) {
        return DRAGDROP_S_DROP;
      }
      let Some(deferred) = &self.deferred else {
        return DRAGDROP_S_DROP;
      };
      match download_now(deferred) {
        Ok(()) => DRAGDROP_S_DROP,
        Err(error) => {
          if !matches!(error, VError::Cancelled) {
            *self.failure.lock() = Some(error);
          }
          DRAGDROP_S_CANCEL
        }
      }
    }
  }

  impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, escape_pressed: BOOL, key_state: MODIFIERKEYS_FLAGS) -> HRESULT {
      if escape_pressed.as_bool() {
        return DRAGDROP_S_CANCEL;
      }
      if key_state.0 & MK_LBUTTON.0 != 0 {
        return S_OK;
      }
      if self.last_effect.get() == DROPEFFECT_NONE || cursor_over_window(self.owner_window) {
        return DRAGDROP_S_CANCEL;
      }
      self.render_before_drop()
    }

    fn GiveFeedback(&self, effect: DROPEFFECT) -> HRESULT {
      self.last_effect.set(effect);
      DRAGDROP_S_USEDEFAULTCURSORS
    }
  }

  fn download_now(deferred: &DeferredDownload) -> VResult<()> {
    deferred.queue.add(deferred.requests.clone())?;
    loop {
      pump_pending_messages();
      let status = deferred.queue.batch_status(&deferred.batch);
      if status.failed > 0 {
        deferred.queue.remove_batch(&deferred.batch);
        return Err(VError::Other(
          "download failed before the drop; see the transfer queue".into(),
        ));
      }
      if status.pending == 0 {
        return Ok(());
      }
      if escape_is_down() {
        deferred.queue.remove_batch(&deferred.batch);
        return Err(VError::Cancelled);
      }
      unsafe {
        MsgWaitForMultipleObjectsEx(
          None,
          POLL_INTERVAL.as_millis() as u32,
          QS_ALLINPUT,
          MWMO_INPUTAVAILABLE,
        );
      }
    }
  }

  fn pump_pending_messages() {
    let mut message = MSG::default();
    unsafe {
      while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
      }
    }
  }

  fn cursor_over_window(owner: isize) -> bool {
    if owner == 0 {
      return false;
    }
    unsafe {
      let mut point = POINT::default();
      if GetCursorPos(&mut point).is_err() {
        return false;
      }
      let under = WindowFromPoint(point);
      if under.is_invalid() {
        return false;
      }
      GetAncestor(under, GA_ROOT).0 as isize == owner
    }
  }

  fn escape_is_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16) & 0x8000 != 0 }
  }
}

#[cfg(not(windows))]
mod platform {
  use super::{DragJob, DragOutResult};
  use crate::error::{VError, VResult};
  use tauri::AppHandle;
  use tokio::sync::oneshot;

  pub fn install(_app: &AppHandle) {}

  pub fn post(_job: DragJob, _done: oneshot::Sender<VResult<DragOutResult>>) -> VResult<()> {
    Err(VError::Unsupported(
      "native drag out is only available on Windows".into(),
    ))
  }
}

#[cfg(windows)]
pub mod hglobal {
  use crate::error::{VError, VResult};
  use windows::Win32::Foundation::{HGLOBAL, POINT};
  use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
  };
  use windows::Win32::UI::Shell::DROPFILES;

  pub fn drop_files(paths: &[String]) -> VResult<HGLOBAL> {
    let mut names: Vec<u16> = Vec::new();
    for path in paths {
      names.extend(path.encode_utf16());
      names.push(0);
    }
    names.push(0);
    let header_size = std::mem::size_of::<DROPFILES>();
    let header = DROPFILES {
      pFiles: header_size as u32,
      pt: POINT::default(),
      fNC: false.into(),
      fWide: true.into(),
    };
    let mut bytes = Vec::with_capacity(header_size + names.len() * 2);
    let header_bytes =
      unsafe { std::slice::from_raw_parts(&header as *const DROPFILES as *const u8, header_size) };
    bytes.extend_from_slice(header_bytes);
    for unit in names {
      bytes.extend_from_slice(&unit.to_le_bytes());
    }
    from_bytes(&bytes)
  }

  pub fn dword(value: u32) -> VResult<HGLOBAL> {
    from_bytes(&value.to_le_bytes())
  }

  pub fn text(text: &str) -> VResult<HGLOBAL> {
    let mut bytes = Vec::with_capacity((text.len() + 1) * 2);
    for unit in text.encode_utf16().chain(std::iter::once(0)) {
      bytes.extend_from_slice(&unit.to_le_bytes());
    }
    from_bytes(&bytes)
  }

  pub fn from_bytes(bytes: &[u8]) -> VResult<HGLOBAL> {
    unsafe {
      let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len())
        .map_err(|e| VError::Other(format!("cannot allocate global memory: {e}")))?;
      let target = GlobalLock(handle);
      if target.is_null() {
        return Err(VError::Other("cannot lock global memory".into()));
      }
      std::ptr::copy_nonoverlapping(bytes.as_ptr(), target as *mut u8, bytes.len());
      let _ = GlobalUnlock(handle);
      Ok(handle)
    }
  }

  pub fn read_text(handle: HGLOBAL) -> Option<String> {
    unsafe {
      let size = GlobalSize(handle) / 2;
      let source = GlobalLock(handle) as *const u16;
      if source.is_null() {
        return None;
      }
      let units = std::slice::from_raw_parts(source, size);
      let length = units.iter().position(|&u| u == 0).unwrap_or(size);
      let text = String::from_utf16_lossy(&units[..length]);
      let _ = GlobalUnlock(handle);
      Some(text)
    }
  }

  pub fn read_dword(handle: HGLOBAL) -> Option<u32> {
    unsafe {
      let source = GlobalLock(handle);
      if source.is_null() {
        return None;
      }
      let value = std::ptr::read_unaligned(source as *const u32);
      let _ = GlobalUnlock(handle);
      Some(value)
    }
  }
}

#[cfg(all(test, windows))]
mod tests {
  use super::hglobal;
  use windows::Win32::System::Com::{
    IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
  };
  use windows::Win32::System::Ole::{OleInitialize, CF_HDROP};
  use windows::Win32::UI::Shell::{DragQueryFileW, SHCreateDataObject, HDROP};

  fn drop_format() -> FORMATETC {
    FORMATETC {
      cfFormat: CF_HDROP.0,
      ptd: std::ptr::null_mut(),
      dwAspect: DVASPECT_CONTENT.0,
      lindex: -1,
      tymed: TYMED_HGLOBAL.0 as u32,
    }
  }

  #[test]
  fn drop_files_round_trip_through_shell_data_object() {
    let paths = vec![
      r"C:\Temp\relatório.txt".to_string(),
      r"C:\Temp\pasta".to_string(),
    ];
    unsafe {
      let _ = OleInitialize(None);
      let data: IDataObject = SHCreateDataObject(None, None, None).expect("data object");
      let medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 {
          hGlobal: hglobal::drop_files(&paths).expect("hglobal"),
        },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
      };
      data
        .SetData(&drop_format(), &medium, true)
        .expect("set data");
      let read = data.GetData(&drop_format()).expect("get data");
      let drop = HDROP(read.u.hGlobal.0);
      let count = DragQueryFileW(drop, u32::MAX, None);
      assert_eq!(count, 2);
      let mut names = Vec::new();
      for index in 0..count {
        let length = DragQueryFileW(drop, index, None) as usize;
        let mut buffer = vec![0u16; length + 1];
        DragQueryFileW(drop, index, Some(&mut buffer));
        names.push(String::from_utf16_lossy(&buffer[..length]));
      }
      assert_eq!(names, paths);
    }
  }

  #[test]
  fn dword_global_reads_back() {
    let handle = hglobal::dword(2).expect("hglobal");
    assert_eq!(hglobal::read_dword(handle), Some(2));
  }
}
