package dev.linsheng.muster.app

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.launch
import java.security.Signature
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlin.coroutines.suspendCoroutine

class MusterApplication : Application()

class MainActivity : FragmentActivity() {
    private val model: MusterModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        model.presenceAuthorizer = ::authorize
        model.localAuthenticator = ::authenticate
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) = model.foreground()
            override fun onStop(owner: LifecycleOwner) = model.background()
        })
        intent?.data?.let(::browserLink)
        // The half-fold workspace: the hinge from Jetpack WindowManager while
        // started, and the IME's height from the window's insets.
        lifecycleScope.launch {
            repeatOnLifecycle(androidx.lifecycle.Lifecycle.State.STARTED) { model.display.track(this@MainActivity) }
        }
        val density = resources.displayMetrics.density.toDouble()
        ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { view, insets ->
            model.display.keyboard(insets.getInsets(WindowInsetsCompat.Type.ime()).bottom / density)
            ViewCompat.onApplyWindowInsets(view, insets)
        }
        setContent {
            val url by model.openUrl.collectAsState()
            LaunchedEffect(url) {
                url?.let {
                    runCatching { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(it))) }
                    model.openUrl.value = null
                }
            }
            MusterTheme { MusterRoot(model) }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let(::browserLink)
    }

    /** muster://browser?device=…&request=… from the Chrome extension's setup page. */
    private fun browserLink(uri: Uri) {
        if (uri.scheme == "muster" && uri.host == "browser") model.openBrowserLink(uri)
    }

    /** The biometric prompt around the presence key's Signature. */
    private suspend fun authorize(signature: Signature, reason: String): Signature = suspendCoroutine { cont ->
        val prompt = BiometricPrompt(this, ContextCompat.getMainExecutor(this), object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                val authorized = result.cryptoObject?.signature
                if (authorized != null) cont.resume(authorized) else cont.resumeWithException(IllegalStateException("no crypto object"))
            }

            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                cont.resumeWithException(IllegalStateException(errString.toString()))
            }
        })
        prompt.authenticate(promptInfo(reason), BiometricPrompt.CryptoObject(signature))
    }

    /** The biometric prompt alone (the LocalAuth effect): the answer is yes or no. */
    private suspend fun authenticate(reason: String): Unit = suspendCoroutine { cont ->
        val prompt = BiometricPrompt(this, ContextCompat.getMainExecutor(this), object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = cont.resume(Unit)

            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                cont.resumeWithException(IllegalStateException(errString.toString()))
            }
        })
        prompt.authenticate(promptInfo(reason))
    }

    private fun promptInfo(reason: String): BiometricPrompt.PromptInfo = BiometricPrompt.PromptInfo.Builder()
        .setTitle("Muster")
        .setSubtitle(reason)
        .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL)
        .build()
}
