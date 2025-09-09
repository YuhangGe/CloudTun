package com.cloudv2ray.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
    super.onActivityResult(requestCode, resultCode, data)
    if (requestCode == 0x9999 && resultCode == RESULT_OK) {
      val intent = Intent(this, CloudV2RayVpnService::class.java)
      startForegroundService(intent)
    }
  }
}
