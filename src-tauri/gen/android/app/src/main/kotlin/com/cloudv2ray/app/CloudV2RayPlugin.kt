package com.cloudv2ray.app

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
 
@InvokeArg
class StartVpnArgs {
  var config: String? = null
}

@TauriPlugin
class CloudV2RayPlugin(private val activity: Activity): Plugin(activity) {
  
  @Command
  fun startVpn(invoke: Invoke) {
    val args = invoke.parseArgs(StartVpnArgs::class.java)
    println("startVpn: ${args.config}")
//    val context = activity.applicationContext
    val p = VpnService.prepare(activity)
    if (p != null) {
      activity.startActivityForResult(p, 0x9999)
    } else {
      val intent = Intent(activity, CloudV2RayVpnService::class.java)
      activity.startForegroundService(intent)
    }
  
    println("after startVpn")
    val ret = JSObject()
    ret.put("success", true)
    invoke.resolve(ret)
  }
}