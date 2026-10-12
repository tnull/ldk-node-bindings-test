pluginManagement {
    repositories {
        val mirror = System.getenv("LDK_MAVEN_REPOSITORY")
        if (mirror != null) maven { url = uri(mirror) }
        else { gradlePluginPortal(); google(); mavenCentral() }
    }
}
rootProject.name = "ldk-node"
include("lib")
