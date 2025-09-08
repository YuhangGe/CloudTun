package com.cloudv2ray.app

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@TauriPlugin
class CloudV2RayPlugin(private val activity: Activity): Plugin(activity) {
  
  @Command
  fun startVpn(invoke: Invoke) {
    
  }
}