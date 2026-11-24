package com.cloudtun.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.VpnService
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import org.json.JSONObject
import java.util.Locale

class CloudTunVpnService : VpnService() {
  companion object {
    var instance: CloudTunVpnService? = null
    @Volatile var isVpnConnected = false
    var startCallback: ((String?) -> Unit)? = null
    private const val RUNNING_CHANNEL = "vpn_service_channel"
    private const val EVENT_CHANNEL = "vpn_events"
    private const val RUNNING_ID = 1
    private const val EVENT_ID = 2
  }
  private var vpnInterface: ParcelFileDescriptor? = null
  private val vpn = CloudTunVpn()
  private val handler = Handler(Looper.getMainLooper())
  private var wakeLock: PowerManager.WakeLock? = null
  private var lastEventId = 0L
  private var lastNotification = ""
  private val traffic = VpnTrafficDisplay()
  private val healthCheck = object : Runnable {
    override fun run() {
      if (!isVpnConnected) return
      try {
        val raw = vpn.status()
        if (raw == "null") {
          disconnect()
          showEvent("VPN 已停止", "代理内核已退出，请重新连接")
          return
        }
        val status = JSONObject(raw)
        if (status.optString("eventKind") == "cloudLost") {
          val message = status.optString("eventMessage")
          disconnect()
          showEvent("云主机实例已丢失", message, 5000)
          return
        }
        updateWakeLock(status.optBoolean("autoSwitch"))
        val sample = vpn.stats()
        val trafficText = if (sample == "null") "速率 0 B/s · 累计 0 B" else JSONObject(sample).let {
          traffic.update(it.optLong("sessionId"), it.optLong("elapsedMs"), it.optLong("totalBytes"))
        }
        val name = connectionName(status)
        val state = when {
          status.optBoolean("switching") -> "正在检测并切换节点…"
          !status.isNull("error") -> status.optString("error")
          else -> "VPN 运行中"
        }
        val fingerprint = "$name|$trafficText|$state"
        if (fingerprint != lastNotification && manager().areNotificationsEnabled()) {
          manager().notify(RUNNING_ID, runningNotification(name, trafficText, state))
          lastNotification = fingerprint
        }
        val eventId = status.optLong("eventId")
        if (eventId > lastEventId) {
          lastEventId = eventId
          val kind = status.optString("eventKind")
          val title = if (kind == "unavailable") "暂无可用节点" else if (kind == "recovered") "连接已恢复" else "已自动切换节点"
          showEvent(title, status.optString("eventMessage"))
          Log.i("CloudTunVpnService", "$title: ${status.optString("eventMessage")}")
        }
      } catch (error: Exception) {
        Log.w("CloudTunVpnService", "Cannot refresh VPN notification", error)
      }
      if (isVpnConnected) handler.postDelayed(this, 2000)
    }
  }
  private fun manager() = getSystemService(NotificationManager::class.java)
  private fun openIntent(): PendingIntent = PendingIntent.getActivity(this, 0,
    Intent(this, MainActivity::class.java), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
  private fun connectionName(status: JSONObject): String =
    if (status.optString("source") == "cloud") "云主机：${status.optString("name")}" else "节点：${status.optString("name")}"
  private fun runningNotification(name: String, traffic: String, state: String): Notification {
    val stop = PendingIntent.getService(this, 0, Intent(this, CloudTunVpnService::class.java).setAction("STOP"), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    return NotificationCompat.Builder(this, RUNNING_CHANNEL)
      .setContentTitle("CloudTun · $name")
      .setContentText(traffic)
      .setStyle(NotificationCompat.BigTextStyle().bigText("$name\n$traffic\n$state"))
      .setSubText(state)
      .setSmallIcon(R.drawable.ic_stat_vpn)
      .setContentIntent(openIntent())
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setShowWhen(false)
      .setCategory(NotificationCompat.CATEGORY_SERVICE)
      .addAction(R.drawable.ic_stat_vpn, "断开连接", stop)
      .build()
  }
  private fun showEvent(title: String, message: String, timeoutMs: Long = 0) {
    if (!manager().areNotificationsEnabled()) return
    try {
      manager().notify(EVENT_ID, NotificationCompat.Builder(this, EVENT_CHANNEL)
        .setContentTitle("CloudTun · $title").setContentText(message)
        .setStyle(NotificationCompat.BigTextStyle().bigText(message))
        .setSmallIcon(R.drawable.ic_stat_vpn).setContentIntent(openIntent())
        .setAutoCancel(true).setTimeoutAfter(timeoutMs).build())
    } catch (error: SecurityException) { Log.w("CloudTunVpnService", "Notification permission unavailable") }
  }
  private fun updateWakeLock(enabled: Boolean) {
    if (enabled && wakeLock?.isHeld != true) {
      wakeLock = getSystemService(PowerManager::class.java).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "CloudTun:Failover").apply {
        setReferenceCounted(false)
        acquire()
      }
    } else if (!enabled && wakeLock?.isHeld == true) {
      wakeLock?.release()
      wakeLock = null
    }
  }
  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action != "START") { disconnect(); return START_NOT_STICKY }
    try {
      instance = this
      check(vpnInterface == null) { "VPN 服务已启动" }
      manager().createNotificationChannel(NotificationChannel(RUNNING_CHANNEL, "CloudTun VPN", NotificationManager.IMPORTANCE_LOW))
      manager().createNotificationChannel(NotificationChannel(EVENT_CHANNEL, "CloudTun 连接状态", NotificationManager.IMPORTANCE_DEFAULT))
      traffic.reset()
      lastEventId = 0
      lastNotification = ""
      manager().cancel(EVENT_ID)
      ServiceCompat.startForeground(this, RUNNING_ID, runningNotification("正在连接", "速率 0 B/s · 累计 0 B", "启动 VPN…"), ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
      val builder = Builder().setSession("CloudTun").addAddress("10.0.0.2", 24)
        .addRoute("0.0.0.0", 0).addDnsServer("198.18.0.2").setMtu(1500)
      builder.addDisallowedApplication(packageName)
      vpnInterface = checkNotNull(builder.establish()) { "未获得 VPN 授权" }
      val config = checkNotNull(intent.getStringExtra("config")) { "缺少 VPN 配置" }
      val result = try { vpn.start(vpnInterface!!.fd, 1500, config) } finally { java.io.File(config).delete() }
      check(result == 0) { "启动 VPN 内核失败 ($result)" }
      isVpnConnected = true
      handler.post(healthCheck)
      finishStart(null)
    } catch (error: Exception) {
      Log.e("CloudTunVpnService", "VPN startup failed", error)
      closeVpn()
      finishStart(error.message ?: "启动 VPN 失败")
      stopSelf()
    }
    return START_NOT_STICKY
  }
  private fun finishStart(error: String?) { val callback = startCallback; startCallback = null; callback?.invoke(error) }
  fun disconnect() { val connected = isVpnConnected; closeVpn(); if (connected) manager().cancel(EVENT_ID); stopSelf() }
  private fun closeVpn() {
    handler.removeCallbacks(healthCheck)
    isVpnConnected = false
    updateWakeLock(false)
    if (vpnInterface != null) vpn.stop()
    vpnInterface?.close()
    vpnInterface = null
    stopForeground(STOP_FOREGROUND_REMOVE)
  }
  override fun onRevoke() { disconnect(); super.onRevoke() }
  override fun onDestroy() { closeVpn(); if (instance === this) instance = null; finishStart("VPN 服务已停止"); super.onDestroy() }
}

/** Combined upload/download payload bytes, sampled once every two seconds. */
internal class VpnTrafficDisplay {
  private var session = -1L
  private var elapsed = 0L
  private var bytes = 0L
  fun reset() { session = -1; elapsed = 0; bytes = 0 }
  fun update(nextSession: Long, nextElapsed: Long, totalBytes: Long): String {
    val nextBytes = totalBytes.coerceAtLeast(0)
    val rate = if (nextSession == session && nextElapsed > elapsed && nextBytes >= bytes)
      (nextBytes - bytes).toDouble() * 1000 / (nextElapsed - elapsed) else 0.0
    session = nextSession; elapsed = nextElapsed; bytes = nextBytes
    return "速率 ${format(rate)}/s · 累计 ${format(nextBytes.toDouble())}"
  }
  private fun format(value: Double): String {
    val units = arrayOf("B", "KB", "MB", "GB", "TB")
    var amount = value; var index = 0
    while (amount >= 1000 && index < units.lastIndex) { amount /= 1000; index++ }
    return if (index == 0) "${amount.toLong()} ${units[index]}" else String.format(Locale.US, "%.1f %s", amount, units[index])
  }
}
