package dev.linsheng.muster.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import dev.linsheng.muster.app.input.MusterTextField
import dev.linsheng.muster.core.MusterCore
import muster.app.ViewModel

/**
 * The Chrome extension (Agent Halo Browser) on one device: its setup, and
 * the ad-hoc requests the core forwards to it. The target decides what the
 * extension offers and whether a request is allowed; this screen shows the
 * core's status and result and collects the request text.
 */
@Composable
fun BrowserScreen(core: MusterCore, vm: ViewModel) {
    val browser = vm.browser
    val status = browser.status
    val busy = browser.busy
    var request by remember { mutableStateOf(browser.draft) }
    // A changed draft (a browser link the core received) replaces what is being typed.
    LaunchedEffect(browser.draft) { request = browser.draft }
    val secondary = MaterialTheme.colorScheme.onSurfaceVariant
    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize()) {
            item(key = "extension") {
                SectionHeader("Chrome extension")
                Column(Modifier.fillMaxWidth().padding(16.dp, 4.dp, 16.dp, 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (status.connected) {
                            Icon(Icons.Default.CheckCircle, null, Modifier.size(18.dp), tint = Color(0xFF43A047))
                            Text("Connected", color = Color(0xFF43A047))
                        } else {
                            Icon(Icons.Default.Warning, null, Modifier.size(18.dp), tint = secondary)
                            Text("Setup needed", color = secondary)
                        }
                    }
                    Text(status.message.ifEmpty { "Checking the target device…" }, style = MaterialTheme.typography.bodyMedium)
                    if (browser.local) {
                        TextButton(onClick = { core.browserSetup(install = true) }, enabled = !busy) { Text("1. Set up native host") }
                        Text("In Chrome, turn on Developer mode, choose Load unpacked, and select the AgentHaloBrowser folder. Chrome asks for browser debugging permission.",
                            style = MaterialTheme.typography.bodySmall, color = secondary)
                        if (status.extensionPath.isNotEmpty()) {
                            Text(status.extensionPath, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                        }
                    } else {
                        Text("On the target Mac, open Muster → Settings → Chrome extension and complete installation. Then return here and check the connection.",
                            style = MaterialTheme.typography.bodySmall, color = secondary)
                    }
                    TextButton(onClick = { core.browserSetup() }, enabled = !busy) { Text("Check connection") }
                }
            }
            item(key = "access") {
                SectionHeader("Remote access & Locked Use")
                Column(Modifier.fillMaxWidth().padding(16.dp, 4.dp, 16.dp, 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Grant this device browser.view to read pages and browser.control to change them in Network → device → permissions. These permissions are separate from agent access.",
                        style = MaterialTheme.typography.bodySmall, color = secondary)
                    Text("For a locked Mac, also grant locked.control and enable control while locked on the target. Authenticate each remote request. Browser operations keep the Mac locked; no screen cover or temporary unlock is needed.",
                        style = MaterialTheme.typography.bodySmall, color = secondary)
                }
            }
            item(key = "request") {
                SectionHeader("Browser request")
                Column(Modifier.fillMaxWidth().padding(16.dp, 4.dp, 16.dp, 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlinedButton(onClick = { request = "{\"op\":\"tabs\"}" }, enabled = !busy) { Text("List tabs") }
                        OutlinedButton(onClick = { request = "{\"op\":\"ping\"}" }, enabled = !busy) { Text("Connection test") }
                    }
                    MusterTextField(
                        request, { request = it }, Modifier.fillMaxWidth(),
                        placeholder = "{\"op\":\"tabs\"}", enabled = !busy, monospace = true, minHeight = 120.dp, maxHeight = 240.dp,
                    )
                    Text("Use snapshot with a tabId first. Click/type/press also need document and ref from that snapshot.",
                        style = MaterialTheme.typography.bodySmall, color = secondary)
                    Button(onClick = { core.browserRun(request) }, Modifier.fillMaxWidth(), enabled = !busy && request.isNotBlank()) {
                        Text(if (browser.local) "Run" else "Run & authenticate")
                    }
                }
            }
            if (browser.result.isNotEmpty()) {
                item(key = "result") {
                    SectionHeader("Result")
                    Text(browser.result, Modifier.fillMaxWidth().padding(16.dp, 4.dp, 16.dp, 16.dp),
                        fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        if (busy) {
            Surface(Modifier.align(Alignment.Center), shape = RoundedCornerShape(14.dp), tonalElevation = 6.dp) {
                Column(Modifier.padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    CircularProgressIndicator(Modifier.size(32.dp))
                    Text("Waiting for browser…", style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
}
