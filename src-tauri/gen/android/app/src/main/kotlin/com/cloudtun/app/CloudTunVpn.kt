package com.cloudtun.app

class CloudTunVpn {

  companion object {
    init {
      System.loadLibrary("cloudtunvpn")
    }
  }

  private external fun run(tunFd: Int, mtu: Int, serverIp: String, token: String, cvmId: String)
  private external fun stop()
 
  fun startVpn(tunFd: Int, mtu: Int, serverIp: String, token: String, cvmId: String) {
    run(tunFd, mtu, serverIp, token, cvmId);
  }
  
  fun stopVpn() {
    stop()
  }
}