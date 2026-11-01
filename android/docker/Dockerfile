# Android build and emulator image for RustySound (see docs/ANDROID.md).
#
#   docker build -t rustysound-android-dev android/docker
#
# JDK 17, the Android SDK (platform 35, build-tools 35), the NDK, Rust with
# both Android targets, and dx 0.7.10. The x86_64 emulator and its system
# image are in the last layers, so the build layers stay cached without them.
FROM rust:1-bookworm

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        openjdk-17-jdk-headless unzip curl ca-certificates git \
        libpulse0 libgl1 libnss3 libxcomposite1 libxcursor1 libxdamage1 \
        libxi6 libxtst6 libxrandr2 libasound2 libxkbfile1 libdbus-1-3 \
    && rm -rf /var/lib/apt/lists/*

ENV ANDROID_HOME=/opt/android-sdk \
    ANDROID_SDK_ROOT=/opt/android-sdk \
    JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
ENV NDK_HOME=$ANDROID_HOME/ndk/26.3.11579264 \
    ANDROID_NDK_HOME=$ANDROID_HOME/ndk/26.3.11579264 \
    PATH=$PATH:/opt/android-sdk/cmdline-tools/latest/bin:/opt/android-sdk/platform-tools:/opt/android-sdk/emulator

RUN mkdir -p $ANDROID_HOME/cmdline-tools \
    && curl -fsSL -o /tmp/tools.zip https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip \
    && unzip -q /tmp/tools.zip -d $ANDROID_HOME/cmdline-tools \
    && mv $ANDROID_HOME/cmdline-tools/cmdline-tools $ANDROID_HOME/cmdline-tools/latest \
    && rm /tmp/tools.zip \
    && (yes | sdkmanager --licenses >/dev/null || true) \
    && sdkmanager "platform-tools" "platforms;android-35" "build-tools;35.0.0" \
        "ndk;26.3.11579264"

RUN rustup target add aarch64-linux-android x86_64-linux-android \
    && cargo install -j 4 dioxus-cli@0.7.10 --locked \
    && rm -rf /usr/local/cargo/registry

# Emulator (headless, run with --device /dev/kvm).
RUN sdkmanager "emulator" "system-images;android-35;google_apis;x86_64" \
    && echo no | avdmanager create avd -n rs35 -k "system-images;android-35;google_apis;x86_64" -d pixel_6

WORKDIR /work
