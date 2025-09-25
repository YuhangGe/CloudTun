package com.cloudtun.app

import android.app.Activity
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.drawable.BitmapDrawable
import android.net.VpnService
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.ByteArrayOutputStream

@InvokeArg
class StartVpnArgs {
  var serverIp: String? = null
  var token: String? = null
  var proxyApps: String? = null
}

data class AppInfo(
  val appName: String,         // 应用名称
  val appIcon: String,       // 应用图标
  val applicationName: String  // applicationName
)

fun bitmapToBase64Src(bitmap: Bitmap): String {
  val outputStream = ByteArrayOutputStream()
  bitmap.compress(Bitmap.CompressFormat.PNG, 100, outputStream) // 压缩成 PNG 格式
  val byteArray = outputStream.toByteArray()
  return "data:image/png;base64,${Base64.encodeToString(byteArray, Base64.DEFAULT)}"
}

@TauriPlugin
class CloudTunPlugin(private val activity: Activity): Plugin(activity) {

//  companion object {
//    var activityResultCallback: ((requestCode: Int, resultCode: Int, data: Intent?) -> Unit)? = null
//  }
  
  @Command
  fun listAllApps(invoke: Invoke) {
    val packageManager = activity.packageManager
    val queryIntent = Intent(Intent.ACTION_MAIN, null)
    queryIntent.addCategory(Intent.CATEGORY_LAUNCHER)
    val activities = packageManager.queryIntentActivities(queryIntent, 0)
 
    val arr = JSArray()
    
     activities.forEach { k -> 
      val info = k.activityInfo.applicationInfo 
       val icon = (info.loadIcon(packageManager) as? BitmapDrawable)?.bitmap
//      var icon = (packageManager.getApplicationIcon(info) as? BitmapDrawable)?.bitmap;
//       if (icon == null) {
//         icon = (packageManager.getApplicationLogo(info) as? BitmapDrawable)?.bitmap;
//       }
      val name = packageManager.getApplicationLabel(k.activityInfo.applicationInfo).toString()
         val item = JSObject()
       item.put("name", name)
       item.put("icon", if (icon == null) "" else bitmapToBase64Src(icon) )
       item.put("id", info.packageName)
       arr.put(item)
    }
    val ret = JSObject()
    ret.put("apps", arr)
    invoke.resolve(ret)
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
      p.putExtra("proxyApps", args.proxyApps)
      activity.startActivityForResult(p, 0x9999)
      println("after startVpn")
      val ret = JSObject()
      ret.put("success", true)
      invoke.resolve(ret)
    } else {
      val intent = Intent(activity, CloudTunVpnService::class.java).apply {
        action = "START"
        putExtra("serverIp", args.serverIp)
        putExtra("token", args.token)
        putExtra("proxyApps", args.proxyApps)
      }
      activity.startService(intent)
      println("after startVpn")
      val ret = JSObject()
      ret.put("success", true)
      invoke.resolve(ret)
    }
  } 
  
  @Command
  fun stopVpn(invoke: Invoke) {
    val intent = Intent(activity, CloudTunVpnService::class.java).apply {
       action = "STOP"
    }
    activity.startService(intent)
    invoke.resolve()
  }
  
  @Command
  fun getVpnConnected(invoke: Invoke) {
    val ret = JSObject()
    ret.put("connected", CloudTunVpnService.isVpnConnected)
    invoke.resolve(ret)
  }
 
}