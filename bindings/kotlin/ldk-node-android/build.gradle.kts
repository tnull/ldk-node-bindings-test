buildscript {
    repositories {
        val mirror = System.getenv("LDK_MAVEN_REPOSITORY")
        if (mirror != null) maven { url = uri(mirror) }
        else { google(); mavenCentral() }
    }
    dependencies {
        classpath("com.android.tools.build:gradle:7.1.2")
    }
}

plugins {
}

// library version is defined in gradle.properties
val libraryVersion: String by project

group = "org.lightningdevkit"
version = libraryVersion
