package com.cloudtun.app

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
      val intent = Intent(this, CloudTunVpnService::class.java)
      if (data != null) {
        intent.putExtra("serverIp", data.getStringExtra("serverIp"))
        intent.putExtra("token", data.getStringExtra("token"))
      }
      startService(intent)
    }
  }
}
