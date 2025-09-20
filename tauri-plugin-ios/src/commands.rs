use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::IosExt;
use crate::Result;

#[command]
pub(crate) async fn tauri_start_ios_proxy<R: Runtime>(
  app: AppHandle<R>,
  payload: PingRequest,
) -> Result<PingResponse> {
  app.ios().start_proxy(payload)
}

#[command]
pub(crate) async fn tauri_stop_ios_proxy<R: Runtime>(
  app: AppHandle<R>,
  payload: PingRequest,
) -> Result<PingResponse> {
  app.ios().stop_proxy(payload)
}
