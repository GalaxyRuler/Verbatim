import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

val sherpaOnnxLibDir = System.getenv("SHERPA_ONNX_LIB_DIR")
val sherpaOnnxAndroidAbi = System.getenv("SHERPA_ONNX_ANDROID_ABI")
    ?: inferSherpaOnnxAndroidAbi(sherpaOnnxLibDir)
val stagedSherpaOnnxAndroidAbi = sherpaOnnxAndroidAbi ?: "unused"
val stagedSherpaOnnxJniLibsDir = layout.buildDirectory.dir("generated/sherpa-onnx-jniLibs")
val debugKeystoreFile = rootProject.file("debug.keystore")

android {
    compileSdk = 36
    namespace = "com.galaxyruler.verbatim"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.galaxyruler.verbatim"
        minSdk = 26
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        getByName("debug") {
            // Throwaway debug-only key committed so CI/emulator installs share one stable signature.
            storeFile = debugKeystoreFile
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
        }
    }
    buildTypes {
        getByName("debug") {
            signingConfig = signingConfigs.getByName("debug")
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
    testOptions {
        unitTests.isIncludeAndroidResources = true
        unitTests.isReturnDefaultValues = true
    }
    sourceSets {
        getByName("main") {
            jniLibs.srcDir(stagedSherpaOnnxJniLibsDir)
        }
    }
}

val stageSherpaOnnxJniLibs by tasks.registering(Copy::class) {
    onlyIf { !sherpaOnnxLibDir.isNullOrBlank() && !sherpaOnnxAndroidAbi.isNullOrBlank() }
    if (!sherpaOnnxLibDir.isNullOrBlank()) {
        from(file(sherpaOnnxLibDir))
    }
    include("*.so")
    into(stagedSherpaOnnxJniLibsDir.map { it.dir(stagedSherpaOnnxAndroidAbi) })
}

tasks.matching { it.name == "preBuild" }.configureEach {
    dependsOn(stageSherpaOnnxJniLibs)
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    testImplementation("junit:junit:4.13.2")
    // Phase 0 / Task 0.1 (v2): JUnit4 already present; add only MockK + Robolectric.
    testImplementation("io.mockk:mockk:1.13.13")
    testImplementation("org.robolectric:robolectric:4.14.1")
    testImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")

fun inferSherpaOnnxAndroidAbi(libDir: String?): String? {
    if (libDir.isNullOrBlank()) {
        return null
    }

    val normalized = libDir.replace('\\', '/').lowercase()
    return when {
        "arm64" in normalized || "aarch64" in normalized -> "arm64-v8a"
        "armeabi-v7a" in normalized || "armv7" in normalized -> "armeabi-v7a"
        "x86_64" in normalized -> "x86_64"
        Regex("(^|/)x86($|/)").containsMatchIn(normalized) -> "x86"
        else -> null
    }
}
