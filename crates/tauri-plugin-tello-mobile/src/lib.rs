//! Ajustes nativos do Tello Pilot no celular.
//!
//! - Android: prende o app à rede Wi-Fi (o Wi-Fi do Tello não tem internet e o
//!   sistema mandaria o tráfego pela rede móvel), mantém a tela ligada e fixa a
//!   orientação em paisagem.
//! - iOS: impede o bloqueio automático da tela durante o voo.
//!
//! No desktop o plugin não faz nada.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_tello_mobile);

/// Mantém a referência ao plugin nativo enquanto o app estiver aberto.
#[cfg(mobile)]
struct NativePlugin<R: Runtime>(#[allow(dead_code)] tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("tello-mobile")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            let handle = _api.register_android_plugin("io.github.kaikoaugustin.tellomobile", "TelloMobilePlugin")?;
            #[cfg(target_os = "ios")]
            let handle = _api.register_ios_plugin(init_plugin_tello_mobile)?;
            #[cfg(mobile)]
            {
                use tauri::Manager;
                _app.manage(NativePlugin(handle));
            }
            Ok(())
        })
        .build()
}
