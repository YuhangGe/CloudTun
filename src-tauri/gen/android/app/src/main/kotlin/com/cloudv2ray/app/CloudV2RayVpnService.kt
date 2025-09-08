package com.cloudv2ray.app

import android.content.Intent
import android.net.ProxyInfo
import android.net.VpnService
import android.os.ParcelFileDescriptor
import java.io.IOException

class CloudV2RayVpnService : VpnService() {
  private var vpnInterface: ParcelFileDescriptor? = null
  private var v2rayService: V2RayService? = null
  private var isRunning = false



  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    // 初始化 VPN 配置
    val builder = Builder()
    builder.setSession("CloudV2Ray VPN Service")  // 设置 VPN 会话名称
      .addAddress("10.0.0.2", 24)  // 为虚拟网络接口分配 IP 地址
      .addRoute("0.0.0.0", 0)  // 默认路由转发所有流量
      .setBlocking(true)  // 设置为阻塞模式，避免泄漏流量
      .setHttpProxy(ProxyInfo.buildDirectProxy("127.0.0.1", 7891))
    
    try {
      vpnInterface = builder.establish()
      isRunning = true
    } catch (e: IOException) {
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

    v2rayService?.startTun2Socks()
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
