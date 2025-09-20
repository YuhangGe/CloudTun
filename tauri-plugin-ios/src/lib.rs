use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

pub use models::*;

#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(mobile)]
use mobile::Ios;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the ios APIs.
pub trait IosExt<R: Runtime> {
  fn ios(&self) -> &Ios<R>;
}

impl<R: Runtime, T: Manager<R>> crate::IosExt<R> for T {
  fn ios(&self) -> &Ios<R> {
    self.state::<Ios<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("ios")
    .invoke_handler(tauri::generate_handler![
      commands::tauri_start_ios_proxy,
      commands::tauri_stop_ios_proxy
    ])
    .setup(|app, api| {
      println!("xxx init ios plugin");
      #[cfg(mobile)]
      let ios = mobile::init(app, api)?;
      app.manage(ios);
      Ok(())
    })
    .build()
}
