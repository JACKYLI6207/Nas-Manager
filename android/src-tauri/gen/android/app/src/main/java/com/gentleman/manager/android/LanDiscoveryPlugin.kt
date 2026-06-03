package com.gentleman.manager.android

import android.app.Activity
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.Inet4Address
import java.net.URL
import java.util.Collections
import java.util.concurrent.Callable
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/**
 * 實機 Wi‑Fi：MulticastLock + 強制 HTTP 走 Wi‑Fi Network（避免行動數據搶路由）。
 */
@TauriPlugin
class LanDiscoveryPlugin(private val activity: Activity) : Plugin(activity) {

    private var multicastLock: WifiManager.MulticastLock? = null
    private var boundWifiNetwork: Network? = null

    private fun connectivityManager(): ConnectivityManager =
        activity.applicationContext.getSystemService(Context.CONNECTIVITY_SERVICE)
            as ConnectivityManager

    private fun getWifiNetwork(): Network? {
        val cm = connectivityManager()
        for (network in cm.allNetworks) {
            val caps = cm.getNetworkCapabilities(network) ?: continue
            if (caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) {
                return network
            }
        }
        return null
    }

    private fun jsonToJsObject(json: JSONObject): JSObject {
        val o = JSObject()
        val keys = json.keys()
        while (keys.hasNext()) {
            val key = keys.next()
            o.put(key, json.get(key))
        }
        return o
    }

    private fun probeHealthOnNetwork(
        network: Network,
        host: String,
        port: Int,
        timeoutMs: Int,
    ): JSONObject {
        val result = JSONObject()
        result.put("host", host)
        try {
            val url = URL("http://$host:$port/api/v1/health")
            val conn = network.openConnection(url) as HttpURLConnection
            conn.connectTimeout = timeoutMs
            conn.readTimeout = timeoutMs
            conn.requestMethod = "GET"
            conn.useCaches = false
            val code = conn.responseCode
            if (code !in 200..299) {
                result.put("ok", false)
                result.put("errorKind", "http_error")
                result.put("error", "HTTP $code")
                return result
            }
            val text = conn.inputStream.bufferedReader().use { it.readText() }
            val json = JSONObject(text)
            if (!json.optBoolean("ok", false)) {
                result.put("ok", false)
                result.put("errorKind", "bad_response")
                result.put("error", "health ok=false")
                return result
            }
            result.put("ok", true)
            result.put("name", json.optString("app", "Gentleman Manager"))
            result.put("port", port)
            result.put("remoteApi", json.optInt("remote_api", 1))
            return result
        } catch (e: java.net.ConnectException) {
            result.put("ok", false)
            result.put("errorKind", "connection_refused")
            result.put("error", e.message ?: "ConnectException")
            return result
        } catch (e: java.net.SocketTimeoutException) {
            result.put("ok", false)
            result.put("errorKind", "timeout")
            result.put("error", e.message ?: "timeout")
            return result
        } catch (e: java.net.NoRouteToHostException) {
            result.put("ok", false)
            result.put("errorKind", "no_route")
            result.put("error", e.message ?: "NoRouteToHost")
            return result
        } catch (e: java.net.SocketException) {
            val msg = e.message ?: ""
            result.put("ok", false)
            result.put(
                "errorKind",
                if (msg.contains("EPERM") || msg.contains("Binding socket to network")) {
                    "bind_failed"
                } else {
                    "other"
                },
            )
            result.put("error", e.javaClass.simpleName + ": " + msg)
            return result
        } catch (e: Exception) {
            result.put("ok", false)
            result.put("errorKind", "other")
            result.put("error", e.javaClass.simpleName + ": " + (e.message ?: ""))
            return result
        }
    }

    private fun bytesToInt(b: ByteArray): Int =
        ((b[0].toInt() and 0xff) shl 24) or
            ((b[1].toInt() and 0xff) shl 16) or
            ((b[2].toInt() and 0xff) shl 8) or
            (b[3].toInt() and 0xff)

    private fun intToIp(v: Int): String =
        "${v ushr 24 and 0xff}.${v ushr 16 and 0xff}.${v ushr 8 and 0xff}.${v and 0xff}"

    private fun ipv4HostRange(bindIp: String, netmask: String, excludeIp: String): List<String> {
        val ip = Inet4Address.getByName(bindIp).address
        val mask = Inet4Address.getByName(netmask).address
        val ipInt = bytesToInt(ip)
        val maskInt = bytesToInt(mask)
        val network = ipInt and maskInt
        val broadcast = network or maskInt.inv()
        val hosts = mutableListOf<String>()
        var host = network + 1
        while (host < broadcast) {
            val s = intToIp(host)
            if (s != excludeIp) {
                hosts.add(s)
            }
            host++
        }
        return hosts
    }

    @Command
    fun beginLanScan(invoke: Invoke) {
        activity.runOnUiThread {
            try {
                val ret = JSObject()
                val network = getWifiNetwork()
                var bindOk = false
                if (network != null) {
                    bindOk = connectivityManager().bindProcessToNetwork(network)
                    if (bindOk) {
                        boundWifiNetwork = network
                    }
                }
                ret.put("processBindOk", bindOk)

                val wifi =
                    activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
                multicastLock?.let { lock ->
                    if (lock.isHeld) lock.release()
                }
                multicastLock =
                    wifi.createMulticastLock("gentleman-manager-lan-scan").apply {
                        setReferenceCounted(false)
                        acquire()
                    }
                ret.put("multicastOk", true)
                ret.put(
                    "message",
                    when {
                        network == null ->
                            "未找到 Wi‑Fi Network；請連上 Wi‑Fi 並暫關行動數據"
                        bindOk ->
                            "已 bindProcessToNetwork(Wi‑Fi)；App 流量不走 rmnet"
                        else ->
                            "MulticastLock OK，但 bindProcessToNetwork 失敗（部分機型需關閉行動數據）"
                    },
                )
                invoke.resolve(ret)
            } catch (e: Exception) {
                invoke.reject(e.message ?: "無法開始 Wi‑Fi 區網模式")
            }
        }
    }

    @Command
    fun endLanScan(invoke: Invoke) {
        activity.runOnUiThread {
            try {
                if (boundWifiNetwork != null) {
                    connectivityManager().bindProcessToNetwork(null)
                    boundWifiNetwork = null
                }
                releaseMulticastLock()
                invoke.resolve()
            } catch (e: Exception) {
                invoke.reject(e.message ?: "結束 Wi‑Fi 區網模式失敗")
            }
        }
    }

    @InvokeArg
    class ProbeArgs {
        lateinit var host: String
        var port: Int = 8765
        var timeoutMs: Int = 450
    }

    @Command
    fun probeHealthOnWifi(invoke: Invoke) {
        Thread {
            try {
                val args = invoke.parseArgs(ProbeArgs::class.java)
                val network = getWifiNetwork()
                if (network == null) {
                    val ret = JSObject()
                    ret.put("ok", false)
                    ret.put("errorKind", "no_wifi")
                    ret.put("error", "未找到 Wi‑Fi Network（請確認已連 Wi‑Fi）")
                    invoke.resolve(ret)
                    return@Thread
                }
                val json = probeHealthOnNetwork(network, args.host, args.port, args.timeoutMs)
                invoke.resolve(jsonToJsObject(json))
            } catch (e: Exception) {
                invoke.reject(e.message ?: "probe failed")
            }
        }.start()
    }

    @InvokeArg
    class SubnetScanArgs {
        lateinit var bindIp: String
        lateinit var netmask: String
        var port: Int = 8765
        var timeoutMs: Int = 450
    }

    @Command
    fun scanSubnetHealthOnWifi(invoke: Invoke) {
        Thread {
            try {
                val args = invoke.parseArgs(SubnetScanArgs::class.java)
                val log = mutableListOf<String>()
                val network = getWifiNetwork()
                if (network == null) {
                    log.add("未找到 Wi‑Fi Network")
                    val ret = JSObject()
                    val logArr = JSArray()
                    for (line in log) logArr.put(line)
                    ret.put("logLines", logArr)
                    ret.put("found", JSArray())
                    invoke.resolve(ret)
                    return@Thread
                }

                log.add("HTTP 強制綁定 Wi‑Fi Network（避開 rmnet 行動數據）")
                val hosts = ipv4HostRange(args.bindIp, args.netmask, args.bindIp)
                log.add("掃描 ${hosts.size} 個位址 :${args.port}/health（找到即停）…")

                val pool = Executors.newFixedThreadPool(64)
                val found = Collections.synchronizedList(mutableListOf<JSONObject>())
                val stop = java.util.concurrent.atomic.AtomicBoolean(false)
                try {
                    val futures =
                        hosts.map { host ->
                            pool.submit(
                                Callable {
                                    if (stop.get()) return@Callable null
                                    val r =
                                        probeHealthOnNetwork(
                                            network,
                                            host,
                                            args.port,
                                            args.timeoutMs,
                                        )
                                    if (r.optBoolean("ok", false)) {
                                        synchronized(found) {
                                            if (found.isEmpty()) {
                                                found.add(r)
                                                stop.set(true)
                                            }
                                        }
                                    }
                                    null
                                },
                            )
                        }
                    val deadlineMs = System.currentTimeMillis() + 4500L
                    while (System.currentTimeMillis() < deadlineMs) {
                        if (stop.get()) break
                        if (futures.all { it.isDone }) break
                        Thread.sleep(40)
                    }
                } finally {
                    pool.shutdownNow()
                }

                val sampleErrors = linkedMapOf<String, String>()
                val sampleLimit = AtomicInteger(5)

                for ((kind, msg) in sampleErrors) {
                    log.add("  範例 [$kind] $msg")
                }
                if (found.isEmpty() && sampleErrors.isEmpty()) {
                    log.add("  （全部逾時或無回應；可能 AP 隔離或 PC 不在此子網）")
                }
                log.add("完成：找到 ${found.size} 台")

                val foundArr = JSArray()
                for (item in found) {
                    val o = JSObject()
                    o.put("host", item.optString("host"))
                    o.put("name", item.optString("name"))
                    o.put("port", item.optInt("port"))
                    foundArr.put(o)
                }
                val logArr = JSArray()
                for (line in log) logArr.put(line)
                val ret = JSObject()
                ret.put("found", foundArr)
                ret.put("logLines", logArr)
                invoke.resolve(ret)
            } catch (e: Exception) {
                invoke.reject(e.message ?: "scan failed")
            }
        }.start()
    }

    private fun releaseMulticastLock() {
        multicastLock?.let { lock ->
            if (lock.isHeld) lock.release()
        }
        multicastLock = null
    }
}
