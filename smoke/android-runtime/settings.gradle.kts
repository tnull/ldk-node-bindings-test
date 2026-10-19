pluginManagement {
    repositories { google(); mavenCentral(); gradlePluginPortal() }
}
dependencyResolutionManagement {
    repositories {
        exclusiveContent {
            forRepository { maven { url = uri(System.getenv("LDK_ANDROID_CANDIDATE_REPOSITORY")) } }
            filter { includeModule("org.lightningdevkit", "ldk-node-android") }
        }
        google()
        mavenCentral()
    }
}
rootProject.name = "candidate-android-consumer"
include(":app", ":instrumentation")
