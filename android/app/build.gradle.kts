plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.github.polymeilex.neothesia"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.github.polymeilex.neothesia"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.4.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // No real release signing identity exists yet (no keystore
            // secret configured), so reuse the auto-generated debug
            // keystore: sideload-only, not Play Store ready.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    // cargo-ndk places the built Rust cdylib here, per ABI, before Gradle
    // runs: src/main/jniLibs/<abi>/libneothesia.so
    sourceSets["main"].jniLibs.srcDirs("src/main/jniLibs")
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
}
