plugins {
    id("com.android.test")
}

android {
    namespace = "org.lightningdevkit.qualification.test"
    compileSdk = 34
    buildToolsVersion = "34.0.0"
    targetProjectPath = ":app"
    defaultConfig {
        minSdk = 21
        targetSdk = 31
        testApplicationId = "org.lightningdevkit.qualification.test"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    compileOptions {
        isCoreLibraryDesugaringEnabled = false
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    signingConfigs.getByName("debug") {
        storeFile = rootProject.file("build/consumer-debug.keystore")
    }
    buildTypes {
        create("release") {
            isMinifyEnabled = true
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    variantFilter {
        if (buildType.name != "release") ignore = true
    }
}

dependencies {
    implementation("androidx.test.ext:junit:1.1.3")
    implementation("androidx.test:runner:1.4.0")
}
