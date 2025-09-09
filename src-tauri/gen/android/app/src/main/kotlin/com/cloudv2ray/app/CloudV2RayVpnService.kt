package com.cloudv2ray.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ProxyInfo
import android.net.VpnService
import android.os.ParcelFileDescriptor
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import java.io.IOException

const val CHANNEL_ID: String = "vpn_service_channel"
const val NOTIFICATION_ID = 1;
class CloudV2RayVpnService : VpnService() {
  private var vpnInterface: ParcelFileDescriptor? = null
  private var v2rayService: V2RayService? = null
  private var isRunning = false


  private fun startForeground() {
//    val intent = Intent(this, MainActivity::class.java) // 点击通知时跳转的界面
//    val pendingIntent = PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT)
    val channel = NotificationChannel(
      CHANNEL_ID,
      "CloudV2Ray Notification",
      NotificationManager.IMPORTANCE_DEFAULT
    )
    channel.setDescription("CloudV2Ray Notification")
    val manager = getSystemService<NotificationManager?>(NotificationManager::class.java)
    manager.createNotificationChannel(channel)
    
    val notification = NotificationCompat.Builder(this, CHANNEL_ID)
      .setContentTitle("CloudV2Ray")
//      .setSmallIcon(R.drawable.ic_vpn)
      //      .setContentIntent(pendingIntent)
      .setContentText("VPN Service Running")
      .setOngoing(true)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
      .build()
    ServiceCompat.startForeground(this, NOTIFICATION_ID, notification,ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
  }


  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    println("XXX: onStartCommand")
    startForeground()
    println("XXX: prepare")
 
    // 初始化 VPN 配置
    val builder = Builder()
    builder.setSession("CloudV2Ray VPN Service")  // 设置 VPN 会话名称
      .addAddress("10.0.0.2", 24)  // 为虚拟网络接口分配 IP 地址
      .addRoute("0.0.0.0", 0)  // 默认路由转发所有流量
      .setBlocking(true)  // 设置为阻塞模式，避免泄漏流量
      .setHttpProxy(ProxyInfo.buildDirectProxy("127.0.0.1", 7891))
    println("XXX: builder2")
    try {
      println("XXX: builder3")
      vpnInterface = builder.establish()
      println("XXX: builder4")
      isRunning = true
    } catch (e: IOException) {
      println("XXX: builder err $e")
      e.printStackTrace()
      stopV2Ray()
    }
    
    try {
      runV2Ray()
    } catch (e: Exception) {
      stopV2Ray()
    }
  

    return START_STICKY  // 启动服务
  }
 
  private fun runV2Ray() {
 
    v2rayService = V2RayService(
      context = applicationContext,
      isRunningProvider = { isRunning },
      restartCallback = { runV2Ray() }
    )

    v2rayService?.startRunV2Ray()
  }
  
  private fun stopV2Ray() {
    isRunning = false;
    v2rayService?.stopV2Ray();
    v2rayService = null;

    try {
      vpnInterface?.close()
    } catch (e: IOException) {
      e.printStackTrace()
    }
  }
 
  override fun onDestroy() {
    super.onDestroy()
    stopV2Ray();
  }
}
