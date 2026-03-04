use tauri::{
  plugin::{Builder, TauriPlugin},
  Runtime,
};

#[cfg(mobile)]
use tauri::Manager;

pub use models::*;

#[cfg(mobile)]
mod commands;
mod error;
#[cfg(mobile)]
mod mobile;
mod models;

pub use error::{Error, Result};

#[cfg(mobile)]
use mobile::Ios;

#[cfg(mobile)]
/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the ios APIs.
pub trait IosExt<R: Runtime> {
  fn ios(&self) -> &Ios<R>;
}

#[cfg(mobile)]
impl<R: Runtime, T: Manager<R>> crate::IosExt<R> for T {
  fn ios(&self) -> &Ios<R> {
    self.state::<Ios<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("ios")
    .invoke_handler(tauri::generate_handler![
      #[cfg(mobile)]
      commands::tauri_start_ios_proxy,
      #[cfg(mobile)]
      commands::tauri_stop_ios_proxy
    ])
    .setup(|_app, _api| {
      println!("xxx init ios plugin");
      #[cfg(mobile)]
      {
        let ios = mobile::init(_app, _api)?;
        _app.manage(ios);
      }

      Ok(())
    })
    .build()
}
