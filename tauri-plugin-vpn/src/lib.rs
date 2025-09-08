use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

pub use models::*;

mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

use mobile::Vpn;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the vpn APIs.
pub trait VpnExt<R: Runtime> {
  fn vpn(&self) -> &Vpn<R>;
}

impl<R: Runtime, T: Manager<R>> crate::VpnExt<R> for T {
  fn vpn(&self) -> &Vpn<R> {
    self.state::<Vpn<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("vpn")
    // .invoke_handler(tauri::generate_handler![commands::tauri_start_vpn])
    .setup(|app, api| {
      let vpn = mobile::init(app, api)?;
      app.manage(vpn);
      Ok(())
    })
    .build()
}
