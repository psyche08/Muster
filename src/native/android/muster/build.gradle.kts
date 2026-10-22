// The Android shell of Muster (ARCHITECTURE §3.7): `core` wraps the Rust
// core's C ABI, `app` is Compose around its ViewModel.
plugins {
    id("com.android.application") version "8.7.3" apply false
    id("com.android.library") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.1.0" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.1.0" apply false
    id("com.google.protobuf") version "0.9.4" apply false
}
