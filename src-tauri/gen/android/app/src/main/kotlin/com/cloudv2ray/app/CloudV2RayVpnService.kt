package com.cloudv2ray.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.VpnService
import android.os.ParcelFileDescriptor
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import java.io.File
import java.io.FileOutputStream
import java.io.IOException


const val CHANNEL_ID: String = "vpn_service_channel"
const val NOTIFICATION_ID = 1;
class CloudV2RayVpnService : VpnService() {
  private var vpnInterface: ParcelFileDescriptor? = null
  private var v2rayService: V2RayService? = null
  private var isRunning = false
  
  private external fun TProxyStartService(config_path: String, fd: Int)
  private external fun TProxyStopService()
  private external fun TProxyGetStats(): LongArray
  
  companion object {
    init {
      System.loadLibrary("hev-socks5-tunnel")
    }
  }
 
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
    
 

    // 初始化 VPN 配置
    val builder = Builder()
    builder.setSession("CloudV2Ray VPN Service")  // 设置 VPN 会话名称
      .addAddress("198.18.0.1", 32)  // 为虚拟网络接口分配 IP 地址
      .addRoute("0.0.0.0", 0)  // 默认路由转发所有流量
      .addAddress("fc00::1", 128)
      .addRoute("::", 0)
      .setBlocking(false)
      .setMtu(8500)
      .addDnsServer("8.8.8.8")
      .addDnsServer("2001:4860:4860::8888")
      .addDnsServer("198.18.0.2")
//      .setHttpProxy()

      try {
        val selfName = applicationContext.packageName;
        builder.addDisallowedApplication(selfName)
      } catch (e: Exception) {
        //
      }

      builder.setSession("CloudV2Ray: IPv4 + IPv6 / Global")

//      .setHttpProxy(ProxyInfo.buildDirectProxy("127.0.0.1", 7891))
    println("XXX: builder2")
    try {
      println("XXX: builder3")
      vpnInterface = builder.establish()
      if (vpnInterface == null) {
        stopSelf()
      } else {
        println("XXX: builder4")
        isRunning = true
        runV2Ray(vpnInterface!!.fd)

        startForeground()
      }

    } catch (e: IOException) {
      println("XXX: builder err $e")
      e.printStackTrace()
      stopV2Ray()
    }
//    
    return START_STICKY  // 启动服务
  }
 
 
  private fun runV2Ray(fd: Int) {
// 
//    v2rayService = V2RayService(
//      context = applicationContext,
//      isRunningProvider = { isRunning },
//      restartCallback = { runV2Ray() }
//    )
//
//    v2rayService?.startRunV2Ray()

    val config = """
misc:
  task-stack-size: 81920
tunnel:
  mtu: 8500
socks5:
  port: 7890
  address: 10.0.2.2
  udp: 'udp'
mapdns:
  address: 198.18.0.2
  port: 53
  network: 240.0.0.0
  netmask: 240.0.0.0
  cache-size: 10000
"""
    val x = start(config, fd)
    println("XXX from rust: $x")
//    TProxyStartService(tproxy_file.getAbsolutePath(), tunFd.getFd())
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
