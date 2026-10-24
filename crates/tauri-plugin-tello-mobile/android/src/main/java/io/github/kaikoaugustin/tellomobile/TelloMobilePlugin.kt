package io.github.kaikoaugustin.tellomobile

import android.app.Activity
import android.content.Context
import android.content.pm.ActivityInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import android.util.Log
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import android.webkit.WebView
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Plugin

private const val TAG = "TelloMobile"

@TauriPlugin
class TelloMobilePlugin(private val activity: Activity) : Plugin(activity) {
    private var wifiCallback: ConnectivityManager.NetworkCallback? = null

    override fun load(webView: WebView) {
        activity.runOnUiThread {
            // A tela não pode apagar no meio do voo.
            activity.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
            hideSystemBars()
        }
        bindProcessToWifi()
    }

    /** Tela cheia: as barras do sistema só aparecem ao deslizar da borda. */
    private fun hideSystemBars() {
        val window = activity.window
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.insetsController?.let {
                it.hide(WindowInsets.Type.systemBars())
                it.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                View.SYSTEM_UI_FLAG_FULLSCREEN or
                View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
        }
    }

    /**
     * O Wi-Fi do Tello não tem internet, então o Android passa a usar a rede
     * móvel como padrão e os pacotes UDP para 192.168.10.1 não chegam ao drone.
     * Prender o processo à rede Wi-Fi resolve isso.
     */
    private fun bindProcessToWifi() {
        val cm = activity.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val request = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                val bound = cm.bindProcessToNetwork(network)
                Log.i(TAG, "Wi-Fi disponível; app preso à rede: $bound")
            }

            override fun onLost(network: Network) {
                if (cm.boundNetworkForProcess == network) {
                    cm.bindProcessToNetwork(null)
                    Log.i(TAG, "Wi-Fi perdido; app solto da rede")
                }
            }
        }
        try {
            cm.requestNetwork(request, callback)
            wifiCallback = callback
        } catch (e: SecurityException) {
            Log.e(TAG, "Sem permissão para escolher a rede Wi-Fi", e)
        }
    }
}
