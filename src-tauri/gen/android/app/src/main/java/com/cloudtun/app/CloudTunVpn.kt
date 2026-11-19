package com.cloudtun.app

class CloudTunVpn {

  companion object {
    init {
      System.loadLibrary("cloudtunvpn")
    }
  }

  private external fun run(tunFd: Int, mtu: Int, serverIp: String, cvmId: String, accessToken: String, trustDir: String): Int
  private external fun stop()
 
  fun startVpn(tunFd: Int, mtu: Int, serverIp: String, cvmId: String, accessToken: String, trustDir: String) {
    run(tunFd, mtu, serverIp, cvmId, accessToken, trustDir);
  }
  
  fun stopVpn() {
    stop()
  }
}