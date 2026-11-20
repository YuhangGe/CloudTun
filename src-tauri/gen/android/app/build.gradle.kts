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


android {
    compileSdk = 36
    namespace = "com.cloudtun.app"
    ndkVersion = System.getenv("ANDROID_NDK_HOME")?.let { file(it).name } ?: "29.0.14206865"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "true"
        applicationId = "com.cloudtun.app"
        minSdk = 31
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
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
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")

// Build the standalone VPN JNI library with the same ABI/profile as Tauri.
val vpnTargets = mapOf(
    "Arm64" to ("aarch64-linux-android" to "arm64-v8a"),
    "Arm" to ("armv7-linux-androideabi" to "armeabi-v7a"),
    "X86" to ("i686-linux-android" to "x86"),
    "X86_64" to ("x86_64-linux-android" to "x86_64"),
)
val vpnJni = layout.buildDirectory.dir("generated/vpnJni")
android.sourceSets.getByName("main").jniLibs.srcDir(vpnJni)
for ((arch, targetAbi) in vpnTargets) {
    for (profile in listOf("Debug", "Release")) {
        val vpnBuild = tasks.register<Exec>("vpnBuild$arch$profile") {
            workingDir(rootProject.projectDir.resolve("../../.."))
            commandLine("bash", "scripts/build-android-vpn.sh", targetAbi.first, profile.lowercase(), vpnJni.get().dir(targetAbi.second).asFile.absolutePath, android.ndkDirectory.absolutePath)
        }
        tasks.matching { it.name == "rustBuild$arch$profile" }.configureEach { dependsOn(vpnBuild) }
    }
}
