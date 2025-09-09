package com.cloudv2ray.app

import android.content.Context

import java.io.File

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
            "address": "170.106.148.222",
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

 fun startRunV2Ray() {
   
    val cfgFile = File(context.filesDir, "config.json")
    if (!cfgFile.exists()) {
      cfgFile.writeText(CONFIG)
    }
    val cmd = arrayListOf(
      File(context.applicationInfo.nativeLibraryDir, TUN2SOCKS).absolutePath,
      "-config", cfgFile.absolutePath,
    )
    println("XXX: cmd:${cmd.toString()}")

    try {
      val proBuilder = ProcessBuilder(cmd)
      proBuilder.redirectErrorStream(true)
      process = proBuilder
        .directory(context.filesDir)
        .start()
      Thread {
        println("$TUN2SOCKS check")
        process.waitFor()
        println("$TUN2SOCKS exited")
        if (isRunningProvider()) {
          println("$TUN2SOCKS restart")
          restartCallback()
        }
      }.start()
      println("XXX: $TUN2SOCKS process info: $process")

   
    } catch (e: Exception) {
      println("XXX: Failed to start $TUN2SOCKS process due to $e")
    }
  }
 
 fun stopV2Ray() {
    try {
      println("XXX: $TUN2SOCKS destroy")
      if (::process.isInitialized) {
        process.destroy()
      }
    } catch (e: Exception) {
      println("XXX: Failed to destroy $TUN2SOCKS process due to $e")
    }
  }
}