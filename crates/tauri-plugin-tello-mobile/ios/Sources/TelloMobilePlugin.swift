import Tauri
import UIKit
import WebKit

class TelloMobilePlugin: Plugin {
  public override func load(webview: WKWebView) {
    // A tela não pode bloquear sozinha no meio do voo.
    DispatchQueue.main.async {
      UIApplication.shared.isIdleTimerDisabled = true
    }
  }
}

@_cdecl("init_plugin_tello_mobile")
public func initPlugin() -> Plugin {
  return TelloMobilePlugin()
}
