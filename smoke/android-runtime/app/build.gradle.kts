plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "org.lightningdevkit.qualification"
    compileSdk = 34
    buildToolsVersion = "34.0.0"
    defaultConfig {
        applicationId = "org.lightningdevkit.qualification"
        minSdk = 21
        targetSdk = 31
    }
    compileOptions {
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions { jvmTarget = "1.8" }
    signingConfigs.getByName("debug") {
        storeFile = rootProject.file("build/consumer-debug.keystore")
    }
    buildTypes {
        getByName("release") {
            isMinifyEnabled = true
            signingConfig = signingConfigs.getByName("debug")
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "consumer-entry.pro")
        }
    }
    packagingOptions {
        jniLibs {
            useLegacyPackaging = true
            keepDebugSymbols += "**/*.so"
        }
    }
}

dependencies {
    implementation("org.lightningdevkit:ldk-node-android:${System.getenv("LDK_ANDROID_CANDIDATE_VERSION")}")
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs_nio:2.0.3")
}
