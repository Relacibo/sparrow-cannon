package dev.relacibo.cannon

import android.Manifest
import android.content.pm.PackageManager
import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  // Android 16+ (RESTRICT_LOCAL_NETWORK): Nachfrage beim ersten Start, sonst
  // faellt das AppOp ACCESS_LOCAL_NETWORK auf uid-mode "ignore" und ALLE
  // Pakete der App werden still gedroppt (Timeout/EPERM) — LAN wie Internet.
  // Beide Permissions sind runtime-level; der eBPF-Block hängt an
  // PermissionMonitor.setLocalNetworkPermissions (NEARBY_WIFI_DEVICES).
  private val localNetPermissions =
    registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { _ -> }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    ensureLocalNetworkPermissions()
    // Android 15 erzwingt Edge-to-Edge: WebView-Inhalt unter Status-Bar und
    // Kamera-Loch padsen, damit die UI nicht verdeckt wird. Bei offener
    // Tastatur gewinnt der IME-Insets-Bottom (maxOf) — der WebView-Viewport
    // schrumpft, fokussierte Felder in Modals bleiben sichtbar.
    val root = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(root) { v, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      v.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
      WindowInsetsCompat.CONSUMED
    }
  }

  private fun ensureLocalNetworkPermissions() {
    val perms = arrayOf(
      Manifest.permission.ACCESS_LOCAL_NETWORK,
      Manifest.permission.NEARBY_WIFI_DEVICES,
    )
    val missing = perms.filter {
      ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED
    }
    if (missing.isEmpty()) return
    localNetPermissions.launch(missing.toTypedArray())
  }
}
