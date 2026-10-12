# JNA uses JNI and reflection internally, including desktop-only AWT helpers.
-dontwarn java.awt.**
-keep class com.sun.jna.** { *; }

# Native symbol names, callback methods and structure layouts are an ABI.
# Keep only generated JNA types; ordinary Kotlin API code can still shrink.
-keep interface org.lightningdevkit.ldknode.** extends com.sun.jna.Library { *; }
-keep class org.lightningdevkit.ldknode.** implements com.sun.jna.Callback { *; }
-keep class org.lightningdevkit.ldknode.** extends com.sun.jna.Structure { *; }
-keepattributes RuntimeVisibleAnnotations,AnnotationDefault,InnerClasses,EnclosingMethod
