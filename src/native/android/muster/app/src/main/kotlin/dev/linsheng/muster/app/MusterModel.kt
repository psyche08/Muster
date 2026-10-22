package dev.linsheng.muster.app

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkRequest
import android.net.Uri
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.linsheng.muster.app.display.FoldDisplay
import dev.linsheng.muster.app.input.KeyboardPrefs
import dev.linsheng.muster.core.KeystoreKeys
import dev.linsheng.muster.core.MusterConfig
import dev.linsheng.muster.core.MusterCore
import dev.linsheng.muster.core.MusterKeys
import dev.linsheng.muster.core.SoftwareKeys
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import muster.app.Effect
import muster.app.Lifecycle
import muster.app.ViewModel
import java.net.InetSocketAddress
import java.net.Socket
import java.security.Signature

/**
 * The one object the screens observe: the core's latest ViewModel and the
 * effects the platform is in the middle of. It performs effects and
 * forwards intents; it decides nothing.
 */
class MusterModel(application: Application) : AndroidViewModel(application) {
    val keys: MusterKeys = runCatching { KeystoreKeys.open() }.getOrElse {
        Log.w("muster", "keystore unavailable, software key: $it")
        SoftwareKeys
    }

    val core: MusterCore = MusterCore(
        MusterConfig(
            dir = application.filesDir.resolve("muster").apply { mkdirs() }.absolutePath,
            name = Build.MODEL,
            version = appVersion(application),
        ),
        keys,
    )

    val viewModel: StateFlow<ViewModel> = core.viewModel

    /** Local display state: the half-fold layout preference, the hinge and the keyboard height. */
    val display = FoldDisplay(application)

    /** An effect waiting for the camera. */
    private val _scanRequest = MutableStateFlow<Long?>(null)
    val scanRequest: StateFlow<Long?> = _scanRequest

    /** Local authentication in progress: the reason shown. */
    private val _authenticating = MutableStateFlow<String?>(null)
    val authenticating: StateFlow<String?> = _authenticating

    /** A URL the activity opens, then clears. */
    val openUrl = MutableStateFlow<String?>(null)

    /** Installed by the activity: the biometric prompt around the presence key's Signature. */
    var presenceAuthorizer: (suspend (Signature, String) -> Signature)? = null
        set(value) {
            field = value
            (keys as? KeystoreKeys)?.presenceAuthorizer = value
        }

    /** Installed by the activity: the biometric prompt alone, for the LocalAuth effect. */
    var localAuthenticator: (suspend (String) -> Unit)? = null

    /** A muster://browser link waiting for the roster. */
    private var pendingBrowserLink: Uri? = null

    private val connectivity = application.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) = core.lifecycle(Lifecycle.NETWORK_CHANGED)
        override fun onLost(network: Network) = core.lifecycle(Lifecycle.NETWORK_CHANGED)
    }

    init {
        MusterCore.initLogging(3)
        KeyboardPrefs.open(application)
        viewModelScope.launch {
            // Each effect runs on its own: a network activation must not hold up a scan.
            core.effects.collect { (id, effect) -> launch { perform(id, effect) } }
        }
        viewModelScope.launch { core.viewModel.collect { deliverBrowserLink(it) } }
        connectivity.registerNetworkCallback(NetworkRequest.Builder().build(), networkCallback)
    }

    fun foreground() = core.lifecycle(Lifecycle.FOREGROUND)
    fun background() = core.lifecycle(Lifecycle.BACKGROUND)

    fun scanned(text: String?) {
        val id = _scanRequest.value ?: return
        _scanRequest.value = null
        if (!text.isNullOrEmpty()) core.answer(id, ok = true, text = text) else core.answer(id, ok = false)
    }

    /** muster://browser?device=…&request=…, from the extension's setup page; opened once the roster is known. */
    fun openBrowserLink(uri: Uri) {
        pendingBrowserLink = uri
        deliverBrowserLink(viewModel.value)
    }

    private fun deliverBrowserLink(vm: ViewModel) {
        val uri = pendingBrowserLink ?: return
        if (vm.serviceDevicesList.isEmpty()) return
        pendingBrowserLink = null
        core.openBrowser(device = uri.getQueryParameter("device") ?: "", draft = uri.getQueryParameter("request") ?: "")
    }

    private suspend fun perform(id: Long, effect: Effect) {
        when (effect.efCase) {
            Effect.EfCase.SCAN_QR -> _scanRequest.value = id
            Effect.EfCase.AUTHENTICATE -> {
                _authenticating.value = effect.authenticate.reason
                try {
                    val signature = keys.presenceSign(effect.authenticate.message.toByteArray(), effect.authenticate.reason)
                    core.answer(id, ok = true, data = signature)
                } catch (error: Exception) {
                    core.answer(id, ok = false, error = error.message ?: "authentication failed")
                } finally {
                    _authenticating.value = null
                }
            }
            Effect.EfCase.NOTIFY -> {
                notify(effect.notify.title, effect.notify.body, effect.notify.session)
                core.answer(id, ok = true)
            }
            Effect.EfCase.OPEN_URL -> {
                openUrl.value = effect.openUrl.url
                core.answer(id, ok = true)
            }
            Effect.EfCase.ACTIVATE_NETWORK -> {
                // The core chose the endpoint and deadline; one connect after a
                // foreground return brings the OS network up. The answer is a retry hint.
                val request = effect.activateNetwork
                val result = runCatching { activateNetwork(request.address, request.port, request.timeoutMs) }
                core.answer(id, ok = result.isSuccess, error = result.exceptionOrNull()?.message ?: "")
            }
            Effect.EfCase.LOCAL_AUTH -> {
                val authenticate = localAuthenticator
                if (authenticate == null) {
                    core.answer(id, ok = false, error = "no authentication UI installed")
                    return
                }
                try {
                    authenticate(effect.localAuth.reason)
                    core.answer(id, ok = true)
                } catch (error: Exception) {
                    core.answer(id, ok = false, error = error.message ?: "authentication failed")
                }
            }
            // A phone has no screen cover; the core only asks a desktop's own shell.
            Effect.EfCase.COVER -> core.answer(id, ok = false, error = "no cover on this platform")
            else -> core.answer(id, ok = false, error = "unknown effect")
        }
    }

    private fun notify(title: String, body: String, thread: String) {
        val context = getApplication<Application>()
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Muster", NotificationManager.IMPORTANCE_DEFAULT)
        )
        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle(title)
            .setContentText(body)
            .setGroup(thread)
            .setAutoCancel(true)
            .build()
        runCatching { manager.notify(thread.hashCode(), notification) }
    }

    override fun onCleared() {
        connectivity.unregisterNetworkCallback(networkCallback)
        core.close()
    }

    companion object {
        private const val CHANNEL = "muster"
    }
}

/**
 * Thin OS adapter, as the iOS shell's NetworkActivation: one TCP connect to
 * the endpoint the core chose, within its deadline, so the OS brings the
 * network up. No bytes are sent, and readiness is never a trust decision.
 */
private suspend fun activateNetwork(address: String, port: Int, timeoutMs: Int) = withContext(Dispatchers.IO) {
    require(port in 1..65535) { "Invalid activation port" }
    Socket().use { it.connect(InetSocketAddress(address, port), timeoutMs) }
}

/** This package's "0.1.0 (1)": version name and code, "" when unreadable. */
private fun appVersion(application: Application): String = runCatching {
    val info = application.packageManager.getPackageInfo(application.packageName, 0)
    "${info.versionName} (${info.longVersionCode})"
}.getOrDefault("")
