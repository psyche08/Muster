// dev.linsheng.muster.core: the JNI shell, the Kotlin wrapper, the Keystore
// keys, and the protobuf messages generated from src/muster's schema.
import com.google.protobuf.gradle.id
import com.google.protobuf.gradle.proto

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("com.google.protobuf")
}

android {
    namespace = "dev.linsheng.muster.core"
    compileSdk = 35
    ndkVersion = "27.2.12479018"

    defaultConfig {
        minSdk = 28
        externalNativeBuild {
            cmake {
                arguments += listOf("-DMUSTER_PROFILE=release")
            }
        }
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    sourceSets {
        getByName("main") {
            proto {
                srcDir("../../../../muster/crates/muster-proto/proto")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

protobuf {
    protoc { artifact = "com.google.protobuf:protoc:4.28.3" }
    generateProtoTasks {
        all().forEach { task ->
            task.builtins {
                id("java") { option("lite") }
                id("kotlin") { option("lite") }
            }
        }
    }
}

dependencies {
    api("com.google.protobuf:protobuf-kotlin-lite:4.28.3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    testImplementation("junit:junit:4.13.2")
}
