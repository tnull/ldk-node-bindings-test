package org.lightningdevkit.qualification

import android.content.Context
import java.io.Closeable
import java.net.InetAddress
import java.net.ServerSocket
import java.net.SocketException
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread
import org.lightningdevkit.ldknode.Builder
import org.lightningdevkit.ldknode.LogRecord
import org.lightningdevkit.ldknode.LogWriter
import org.lightningdevkit.ldknode.Network
import org.lightningdevkit.ldknode.defaultConfig

// Adapted from AndroidLibTest.node_start_stop. Keeping this in the application
// means its bindings and callbacks are shrunk with the actual consumer APK.
object CandidateSmoke {
    @JvmStatic
    fun run(context: Context) {
        val pageSize = android.system.Os.sysconf(android.system.OsConstants._SC_PAGESIZE)
        check(pageSize == if (android.os.Build.VERSION.SDK_INT == 35) 16384L else 4096L)
        val callbacks = AtomicInteger()
        val logger = object : LogWriter {
            override fun log(record: LogRecord) {
                android.util.Log.i("CandidateNativeLog", record.args)
                callbacks.incrementAndGet()
            }
        }
        FeeEstimatesServer().use { fees ->
            val nodes = (1..2).map { index ->
                val directory = Files.createTempDirectory(context.cacheDir.toPath(), "ldk-node-$index-")
                val config = defaultConfig()
                config.storageDirPath = directory.toString()
                config.listeningAddresses = listOf("127.0.0.1:${2322 + index}")
                config.network = Network.REGTEST
                val builder = Builder.fromConfig(config)
                builder.setCustomLogger(logger)
                builder.setChainSourceEsplora(fees.url, null)
                builder.build()
            }
            val started = mutableListOf<org.lightningdevkit.ldknode.Node>()
            try {
                nodes.forEach { it.start(); started.add(it) }
                val ids = nodes.map { it.nodeId() }
                check(ids.all { it.length == 66 }) { "Invalid node identifiers: $ids" }
                check(ids[0] != ids[1]) { "Independent nodes share an identifier" }
                nodes.forEach {
                    check(it.onchainPayment().newAddress().startsWith("bcrt1"))
                }
                check(fees.requests.get() >= 2) { "Nodes did not fetch startup fee estimates" }
            } finally {
                started.forEach { it.stop() }
            }
        }
        check(callbacks.get() > 0) { "Native code did not invoke the Kotlin logging callback" }
    }
}

// Node.start fetches fees synchronously. Regtest accepts an empty fee map.
// Other endpoints deliberately fail: this smoke test does not simulate a chain.
private class FeeEstimatesServer : Closeable {
    private val server = ServerSocket(0, 16, InetAddress.getByName("127.0.0.1"))
    val url = "http://127.0.0.1:${server.localPort}"
    val requests = AtomicInteger()
    private val worker = thread(isDaemon = true, name = "regtest-fee-estimates") {
        try {
            while (!server.isClosed) {
                server.accept().use { socket ->
                    socket.soTimeout = 5000
                    val reader = socket.getInputStream().bufferedReader()
                    val request = reader.readLine()
                    while (!reader.readLine().isNullOrEmpty()) { }
                    val status = if (request == "GET /fee-estimates HTTP/1.1") {
                        requests.incrementAndGet()
                        "200 OK"
                    } else {
                        "404 Not Found"
                    }
                    socket.getOutputStream().write(
                        "HTTP/1.1 $status\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".toByteArray()
                    )
                }
            }
        } catch (error: SocketException) {
            if (!server.isClosed) throw error
        }
    }
    override fun close() {
        server.close()
        worker.join(5000)
        check(!worker.isAlive) { "Fee-estimates server did not stop" }
    }
}
