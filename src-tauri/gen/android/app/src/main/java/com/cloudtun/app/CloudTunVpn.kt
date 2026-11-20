package com.cloudtun.app

import androidx.annotation.Keep

@Keep
class CloudTunVpn {
  companion object {
    init { System.loadLibrary("cloudtunvpn") }
  }
  external fun start(tunFd: Int, mtu: Int, config: String): Int
  external fun stop()
  external fun updateRouting(config: String): Int
  external fun stats(): String
  external fun status(): String
  external fun updateFailover(config: String): Int
}
