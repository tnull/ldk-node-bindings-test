import com.android.build.gradle.internal.tasks.DeviceProviderInstrumentTestTask

plugins {
    id("com.android.application") version "7.4.2" apply false
    id("com.android.test") version "7.4.2" apply false
    id("org.jetbrains.kotlin.android") version "1.6.10" apply false
}

gradle.projectsEvaluated {
    val connected = project(":instrumentation").tasks.named<DeviceProviderInstrumentTestTask>("connectedReleaseAndroidTest").get()
    check(!connected.testRunnerFactory.unifiedTestPlatform.get()
        && connected.testRunnerFactory.executionEnum.get().name == "HOST") {
        "Runtime qualification requires AGP's built-in connected test runner"
    }
    println("Confirmed connected release task selects AGP's built-in test runner")
}

val signingKey = layout.buildDirectory.file("consumer-debug.keystore")
val prepareSigningKey = tasks.register<Exec>("prepareSigningKey") {
    outputs.file(signingKey)
    onlyIf { !signingKey.get().asFile.exists() }
    doFirst { signingKey.get().asFile.parentFile.mkdirs() }
    commandLine(
        "${System.getProperty("java.home")}/bin/keytool", "-genkeypair", "-noprompt",
        "-keystore", signingKey.get().asFile, "-storepass", "android", "-keypass", "android",
        "-alias", "androiddebugkey", "-dname", "CN=Android Debug,O=Android,C=US",
        "-keyalg", "RSA", "-keysize", "2048", "-validity", "3650"
    )
}
subprojects {
    tasks.matching { it.name == "validateSigningRelease" }.configureEach {
        dependsOn(prepareSigningKey)
    }
}
