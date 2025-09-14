package com.cloudv2ray.app

import android.app.Activity
import android.content.Intent
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
  var config: String? = null
}
//
//@InvokeArg
//class SetEventHandlerArgs {
//  lateinit var handler: Channel
//}


@TauriPlugin
class CloudV2RayPlugin(private val activity: Activity): Plugin(activity) {
//  private var channel: Channel? = null
  
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
//  
//    startRunV2Ray()
    println("after startVpn")
    val ret = JSObject()
    ret.put("success", true)
    invoke.resolve(ret)
  }
//
//  // This command should not be added to the `build.rs` and exposed as it is only
//  // used internally from the rust backend.
//  @Command
//  fun setEventHandler(invoke: Invoke) {
//    val args = invoke.parseArgs(SetEventHandlerArgs::class.java)
//    this.channel = args.handler
//    invoke.resolve()
//  }
//

  fun startRunV2Ray() {
    
//    val f0 = activity.assets.open("libv2ray_lib.so")
//    val f1 = File(activity.filesDir, "v2ray")
//    f0.copyTo(f1.outputStream())
//    
//    println("XXX: ${f1.exists()}")
//    
//    try {
//      Runtime.getRuntime().exec("chmod 700 ${f1.absolutePath}")
//    } catch (e: Exception) {
//      println("XXX e1: $e")
//    }
//    
//    val f = File(activity.applicationInfo.nativeLibraryDir, "libcloudv2ray_lib.so")
//     
//    println("XXX2: Exists ==> ${f.exists()}")

    val cfgFile = File(activity.filesDir, "config.json")
    if (!cfgFile.exists()) {
      println("XXX: write config")
      cfgFile.writeText(CONFIG)
    } else {
      println("XXX: config exist")
    }
//    
    val cmd = arrayListOf(
      File(activity.applicationInfo.nativeLibraryDir, "libv2ray.so").absolutePath,
      "-config", cfgFile.absolutePath,
    )
    println("XXX: cmd:${cmd.toString()}")

    try {
      val proBuilder = ProcessBuilder(cmd)
      proBuilder.redirectErrorStream(true)
      val process = proBuilder
        .directory(activity.filesDir)
        .start()
      Thread {
        println("XXX check")
        process.waitFor()
        println("XXX exited")
        
      }.start()
      println("XXX: process info: $process")


    } catch (e: Exception) {
      println("XXX: Failed to start process due to $e")
    }
  }

}