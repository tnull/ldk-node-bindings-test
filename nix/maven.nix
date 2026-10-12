{
  pkgs,
  gradle,
  mirror,
  android,
  source,
  platform,
  inputs,
}:
let
  version = inputs.bindings_version;
  androidJnaVersion = inputs.tools.android_jna;
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "ldk-node-${platform}-publication";
  inherit version;
  src = source;
  nativeBuildInputs = [
    gradle
    pkgs.jdk17
    pkgs.unzip
  ];
  JAVA_HOME = "${pkgs.jdk17}/lib/openjdk";
  LDK_MAVEN_REPOSITORY = mirror;
  LDK_RELEASE_MANIFEST = ../release.json;
  SOURCE_DATE_EPOCH = toString inputs.native.source_date_epoch;
  dontConfigure = true;
  dontFixup = true;
  buildPhase = ''
    export GRADLE_USER_HOME="$TMPDIR/gradle"
    export LDK_MAVEN_OUTPUT="$TMPDIR/publication"
    export LC_ALL=C TZ=UTC
    printf '127.0.0.1 localhost %s\n' "$(uname -n)" > "$TMPDIR/hosts"
    export JAVA_TOOL_OPTIONS="-Djdk.net.hosts.file=$TMPDIR/hosts"
    ${pkgs.lib.optionalString (platform == "android") ''
      export ANDROID_HOME=${android}/libexec/android-sdk
      export ANDROID_SDK_ROOT=$ANDROID_HOME
      readelf="$ANDROID_HOME/ndk/${inputs.tools.android_ndk}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf"
      unzip -q ${mirror}/net/java/dev/jna/jna/${androidJnaVersion}/jna-${androidJnaVersion}.aar -d "$TMPDIR/jna"
      for abi in arm64-v8a armeabi-v7a x86_64; do
        bash ${../scripts/build-android-native-check.sh} "$readelf" "$TMPDIR/jna/jni/$abi/libjnidispatch.so"
      done
      printf '\nandroid.aapt2FromMavenOverride=%s/build-tools/${inputs.tools.android_build_tools}/aapt2\n' "$ANDROID_HOME" >> gradle.properties
    ''}
    gradle --offline --no-daemon --no-build-cache --rerun-tasks \
      --max-workers 2 -Pkotlin.compiler.execution.strategy=in-process \
      -Dorg.gradle.caching=false -Dorg.gradle.java.installations.auto-download=false \
      :lib:publishAllPublicationsToCandidateRepository \
      ${
        if platform == "android" then
          ":lib:compileDebugAndroidTestKotlin"
        else
          ":lib:testClasses :lib:test --tests '*NativeSmokeTest'"
      }
    ${pkgs.lib.optionalString (platform == "android") ''
      unzip -q lib/build/outputs/aar/lib-release.aar -d "$TMPDIR/aar"
      for abi in arm64-v8a armeabi-v7a x86_64; do
        bash ${../scripts/build-android-native-check.sh} "$readelf" "$TMPDIR/aar/jni/$abi/libldk_node.so"
      done
    ''}
  '';
  installPhase = ''
    mkdir -p $out
    publication="$LDK_MAVEN_OUTPUT/org/lightningdevkit/ldk-node-${platform}/${version}"
    for extension in ${if platform == "android" then "aar" else "jar"} pom module; do
      cp "$publication/ldk-node-${platform}-${version}.$extension" "$out/"
    done
    cp "$publication/ldk-node-${platform}-${version}-sources.jar" "$out/"
    cp "$publication/ldk-node-${platform}-${version}-javadoc.jar" "$out/"
  '';
}
