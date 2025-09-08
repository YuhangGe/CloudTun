package com.cloudv2ray.app

import android.content.Context
import android.net.LocalSocket
import android.net.LocalSocketAddress
import android.os.ParcelFileDescriptor
import android.util.Log

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import java.io.File
import java.net.Socket

const val CONFIG = """
{
  "log": {
    "loglevel": "error"
  },
  "policy": {
    "levels": {
      "8": {
        "handshake": 4,
        "connIdle": 300,
        "uplinkOnly": 1,
        "downlinkOnly": 1
      }
    },
    "system": {
      "statsOutboundUplink": true,
      "statsOutboundDownlink": true
    }
  },
  "inbounds": [
    {
      "port": 7891,
      "protocol": "http"
    }
  ],
  "outbounds": [
    {
      "protocol": "vmess",
      "settings": {
        "vnext": [
          {
            "address": "43.130.14.193:2080",
            "port": 2080,
            "users": [
              {
                "id": "8542623f-450a-40f5-93f2-5e40843b6f30",
                "alterId": 0
              }
            ]
          }
        ]
      },
      "tag": "proxy"
    },
    {
      "protocol": "freedom",
      "settings": {},
      "tag": "direct"
    },
    {
      "protocol": "blackhole",
      "settings": {},
      "tag": "adblock"
    }
  ],
  "routing": {
    "rules": [
      {
        "domain": [
          "tanx.com",
          "googeadsserving.cn",
          "baidu.com"
        ],
        "type": "field",
        "outboundTag": "adblock"
      },
      {
        "domain": [
          "jd.com",
          "youku.com",
          "baidu.com",
          "bilibili.com"
        ],
        "type": "field",
        "outboundTag": "direct"
      },
      {
        "type": "field",
        "outboundTag": "proxy"
      }
    ]
  }
}
"""

class V2RayService(
  private val context: Context,
  private val isRunningProvider: () -> Boolean,
  private val restartCallback: () -> Unit
) {
  companion object {
    private const val TUN2SOCKS = "libv2ray.so"
  }

  private lateinit var process: Process

 fun startTun2Socks() {
   
    val cfgFile = File(context.filesDir, "config.json")
    if (!cfgFile.exists()) {
      cfgFile.writeText(CONFIG)
    }
    val cmd = arrayListOf(
      File(context.applicationInfo.nativeLibraryDir, TUN2SOCKS).absolutePath,
      "-config", cfgFile.absolutePath,
    )
 
//    Log.i(AppConfig.TAG, cmd.toString())

    try {
      val proBuilder = ProcessBuilder(cmd)
      proBuilder.redirectErrorStream(true)
      process = proBuilder
        .directory(context.filesDir)
        .start()
      Thread {
//        Log.i(AppConfig.TAG, "$TUN2SOCKS check")
        process.waitFor()
//        Log.i(AppConfig.TAG, "$TUN2SOCKS exited")
        if (isRunningProvider()) {
//          Log.i(AppConfig.TAG, "$TUN2SOCKS restart")
          restartCallback()
        }
      }.start()
//      Log.i(AppConfig.TAG, "$TUN2SOCKS process info: $process")

   
    } catch (e: Exception) {
//      Log.e(AppConfig.TAG, "Failed to start $TUN2SOCKS process", e)
    }
  }
 
 fun stopV2Ray() {
    try {
//      Log.i(AppConfig.TAG, "$TUN2SOCKS destroy")
      if (::process.isInitialized) {
        process.destroy()
      }
    } catch (e: Exception) {
//      Log.e(AppConfig.TAG, "Failed to destroy $TUN2SOCKS process", e)
    }
  }
}