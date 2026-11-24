package com.cloudtun.app

import androidx.annotation.Keep
import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.ActivityCallback
import androidx.activity.result.ActivityResult
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@Keep
@InvokeArg
class StartVpnArgs {
  var config: String? = null
}

@Keep
@InvokeArg
class SubscriptionNoticeArgs { var text: String = "" }

@Keep
@TauriPlugin
class CloudTunPlugin(private val activity: Activity): Plugin(activity) {

  @Command
  fun startVpn(invoke: Invoke) {
    val permission = VpnService.prepare(activity)
    if (permission != null) {
      startActivityForResult(invoke, permission, "vpnPermissionResult")
    } else {
      launchVpn(invoke)
    }
  }

  @ActivityCallback
  fun vpnPermissionResult(invoke: Invoke, result: ActivityResult) {
    if (result.resultCode == Activity.RESULT_OK) launchVpn(invoke)
    else invoke.reject("VPN 授权已取消")
  }

  private fun launchVpn(invoke: Invoke) {
    val args = invoke.parseArgs(StartVpnArgs::class.java)
    CloudTunVpnService.startCallback = { error ->
      if (error != null) invoke.reject(error)
      else invoke.resolve(JSObject().put("success", true))
    }
    try {
      ContextCompat.startForegroundService(activity, Intent(activity, CloudTunVpnService::class.java).apply {
        action = "START"
        putExtra("config", args.config)
      })
    } catch (error: Exception) {
      CloudTunVpnService.startCallback = null
      invoke.reject(error.message ?: "启动 VPN 服务失败")
    }
  }

  @Command
  fun requestNotificationPermission(invoke: Invoke) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      if (ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) !=
        PackageManager.PERMISSION_GRANTED) {
        // 请求权限
        ActivityCompat.requestPermissions(
          activity,
          arrayOf(Manifest.permission.POST_NOTIFICATIONS),
          1001
        )
      }
    }
    val ret = JSObject()
    ret.put("success", true)
    invoke.resolve(ret)
  }
  
  @Command
  fun stopVpn(invoke: Invoke) {
    CloudTunVpnService.instance?.disconnect()
    val ret = JSObject()
    ret.put("success", true)
    invoke.resolve(ret)
  }
  
  @Command
  fun updateRouting(invoke: Invoke) {
    val args = invoke.parseArgs(StartVpnArgs::class.java)
    if (CloudTunVpn().updateRouting(args.config ?: "") == 0) invoke.resolve(JSObject().put("success", true))
    else invoke.reject("VPN 分流设置更新失败")
  }

  @Command
  fun updateFailover(invoke: Invoke) {
    val args = invoke.parseArgs(StartVpnArgs::class.java)
    if (CloudTunVpn().updateFailover(args.config ?: "") == 0) invoke.resolve(JSObject().put("success", true))
    else invoke.reject("VPN 自动切换设置更新失败")
  }

  @Command
  fun notifySubscription(invoke: Invoke) {
    val args = invoke.parseArgs(SubscriptionNoticeArgs::class.java)
    CloudTunVpnService.notifySubscription(activity, args.text)
    invoke.resolve(JSObject().put("success", true))
  }
  @Command
  fun clearSubscriptionOutage(invoke: Invoke) {
    CloudTunVpnService.terminalStatus = null
    invoke.resolve(JSObject().put("success", true))
  }

  @Command
  fun getVpnStatus(invoke: Invoke) {
    val raw = CloudTunVpn().status()
    val status = if (raw == "null") CloudTunVpnService.terminalStatus ?: raw else raw
    invoke.resolve(JSObject().put("status", if (status == "null") org.json.JSONObject.NULL else JSObject(status)))
  }

  @Command
  fun getVpnTraffic(invoke: Invoke) {
    val stats = CloudTunVpn().stats()
    val ret = JSObject()
    ret.put("traffic", if (stats == "null") org.json.JSONObject.NULL else JSObject(stats))
    invoke.resolve(ret)
  }

  @Command
  fun getVpnConnected(invoke: Invoke) {
    val ret = JSObject()
    ret.put("connected", CloudTunVpnService.isVpnConnected && CloudTunVpn().stats() != "null")
    invoke.resolve(ret)
  }
 
}