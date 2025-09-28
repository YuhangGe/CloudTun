package com.cloudtun.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.VpnService
import android.os.ParcelFileDescriptor
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import java.io.IOException


const val CHANNEL_ID: String = "vpn_service_channel"
const val NOTIFICATION_ID = 1;
class CloudTunVpnService : VpnService() {
  companion object {
    var isVpnConnected = false
  }
  
  private var vpnInterface: ParcelFileDescriptor? = null
  private var isRunning = false
  
  private val vpn = CloudTunVpn()

  private fun createStopAction(): NotificationCompat.Action {
    // 创建停止 VPN 的 Intent
    val stopIntent = Intent(this, CloudTunVpnService::class.java).apply {
      action = "STOP"
    }

    val stopPendingIntent = PendingIntent.getService(
      this,
      0,
      stopIntent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    )

    return NotificationCompat.Action(
      R.drawable.ic_launcher_foreground, // 停止图标
      "关闭VPN", // Action 文字
      stopPendingIntent
    )
  }

  private fun startForeground() {
//    val intent = Intent(this, MainActivity::class.java) // 点击通知时跳转的界面
//    val pendingIntent = PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT)
    val channel = NotificationChannel(
      CHANNEL_ID,
      "CloudTun Notification",
      NotificationManager.IMPORTANCE_DEFAULT
    ).apply {
      description = "CloudTun Notification"
      setShowBadge(false)   
    }
    val manager = getSystemService<NotificationManager?>(NotificationManager::class.java)
    manager?.createNotificationChannel(channel)
    
    val notification = NotificationCompat.Builder(this, CHANNEL_ID)
      .setContentTitle("CloudTun")
      .setSmallIcon(R.mipmap.ic_launcher)
       //      .setContentIntent(pendingIntent)
      .setContentText("VPN 代理已连接...")
      .setOngoing(true)
      .addAction(createStopAction())
      .setPriority(NotificationCompat.PRIORITY_HIGH)
      .build()
    ServiceCompat.startForeground(this, NOTIFICATION_ID, notification,ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
  }


  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
  
    if (intent != null && intent.action == "STOP") {
      stopForeground(STOP_FOREGROUND_REMOVE)
      stopProxyLoop()
      stopSelf()
      return START_STICKY;
    }
    
    startForeground()
    
    // 初始化 VPN 配置
    val builder = Builder()
    builder.setSession("CloudTun VPN Service")  // 设置 VPN 会话名称
      .addAddress("10.0.0.2", 24)  // 为虚拟网络接口分配 IP 地址
      .addRoute("0.0.0.0", 0)
       .setMtu(1500)
//      .addDnsServer("8.8.8.8")
      .addDnsServer("198.18.0.2")
    
    val proxyApps = intent?.getStringExtra("proxyApps")
    if (proxyApps != null && proxyApps.isNotEmpty()) {
      try {
        proxyApps.split("\n").forEach { pkg ->
          builder.addAllowedApplication(pkg)
          println("XXX add allowed app: $pkg")
        }
      } catch (e: Exception) {
        e.printStackTrace()
      }
    } else {
      try {
        val selfName = applicationContext.packageName;
        builder.addDisallowedApplication(selfName)
        println("addDisallowedApplication $selfName")
      } catch (e: Exception) {
        e.printStackTrace()
      }
    }
   
    builder.setSession("CloudTun: IPv4 / Global")

    val serverIp = intent?.getStringExtra("serverIp")
    val token = intent?.getStringExtra("token")
    val cvmId = intent?.getStringExtra("cvmId")
    if (serverIp == null || token == null || cvmId == null) {
      return START_STICKY
    }
    
//    println("XXX: builder2 $serverIp $token")
    try {
//      println("XXX: builder3")
      vpnInterface = builder.establish()
      if (vpnInterface == null) {
        stopSelf()
      } else {
//        println("XXX: builder4")
        isRunning = true
        isVpnConnected = true
        startProxyLoop(vpnInterface!!.fd, serverIp, token, cvmId)
      }

    } catch (e: IOException) {
//      println("XXX: builder err $e")
      e.printStackTrace()
      stopProxyLoop()
    }
//    
    return START_STICKY  // 启动服务
  }
 
 
  private fun startProxyLoop(fd: Int, serverIp: String, token: String, cvmId: String) {
      Thread {
         try {
           vpn.startVpn(
             fd,
             1500,
             serverIp,
             token,
             cvmId
           )
         } catch (ex: Exception) {
           println("failed vpn thread: $ex")
         }
        println("vpn thread exited")
      }.start()
 
  }
  
  private fun stopProxyLoop() {
    isRunning = false
    isVpnConnected = false
    try {
      vpn.stopVpn()
    } catch (e: Exception) {
      e.printStackTrace()
    }

    try {
      vpnInterface?.close()
    } catch (e: IOException) {
      e.printStackTrace()
    }
  }
 
  override fun onDestroy() {
    super.onDestroy()
    stopForeground(STOP_FOREGROUND_REMOVE)
    stopProxyLoop()
  }
}
