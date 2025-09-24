package com.cloudtun.app

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.AssetManager
import android.net.VpnService
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class StartVpnArgs {
  var serverIp: String? = null
  var token: String? = null
}
 

@TauriPlugin
class CloudTunPlugin(private val activity: Activity): Plugin(activity) {

  companion object {
    var activityResultCallback: ((requestCode: Int, resultCode: Int, data: Intent?) -> Unit)? = null
  }


  @Command
  fun startVpn(invoke: Invoke) {
    val args = invoke.parseArgs(StartVpnArgs::class.java)
    println("startVpn: ${args.serverIp} ${args.token}")
//    val context = activity.applicationContext
    val p = VpnService.prepare(activity)
    if (p != null) {
      
//      activityResultCallback = { requestCode, resultCode, data ->
//        val success = requestCode == 0x9999 && resultCode == Activity.RESULT_OK;
//        if (success) {
//            // 授权成功，启动服务
//            val intent = Intent(activity, CloudTunVpnService::class.java).apply {
//              putExtra("serverIp", args.serverIp)
//              putExtra("token", args.token)
//            }
//            activity.startService(intent)
//        }
//        println("after startVpn")
//        val ret = JSObject()
//        ret.put("success", success)
//        invoke.resolve(ret)
//      }
      p.putExtra("serverIp", args.serverIp)
      p.putExtra("token", args.token)
      activity.startActivityForResult(p, 0x9999)
      println("after startVpn")
      val ret = JSObject()
      ret.put("success", true)
      invoke.resolve(ret)
    } else {
      val intent = Intent(activity, CloudTunVpnService::class.java).apply {
        putExtra("serverIp", args.serverIp)
        putExtra("token", args.token)
      }
      activity.startService(intent)
      println("after startVpn")
      val ret = JSObject()
      ret.put("success", true)
      invoke.resolve(ret)
    }
 
   
  } 
  
  fun stopVpn(invoke: Invoke) {
    invoke.resolve()
  }
  
 
}