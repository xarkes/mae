#!/bin/sh

export ANDROID_HOME="/Users/user/Library/Android/sdk"
export ANDROID_NDK_HOME="${ANDROID_HOME}/ndk"
export PATH="${ANDROID_HOME}/platform-tools:/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin:$PATH"

cargo install cargo-ndk
cargo ndk -t arm64-v8a -o app/src/main/jniLibs/ build || exit 1

./gradlew build
./gradlew installDebug

adb shell am start -n xark.es.mae/.MainActivity
