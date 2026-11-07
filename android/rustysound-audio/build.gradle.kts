// Native Android playback for RustySound: a Media3 MediaSessionService with
// ExoPlayer, driven from Rust over JNI (see docs/ANDROID.md). dx bundles this
// folder as the Gradle module :plugins:rustysoundaudio because of the
// `manganis::ffi` declaration in src/components/audio_manager/native_android_backend.rs.
plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.adarcher.rustysound.audio"
    compileSdk = 35
    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    sourceSets {
        getByName("main") {
            java.srcDirs("src/main/kotlin")
        }
    }
}

dependencies {
    implementation("androidx.media3:media3-exoplayer:1.5.1")
    implementation("androidx.media3:media3-exoplayer-hls:1.5.1")
    implementation("androidx.media3:media3-session:1.5.1")
    implementation("androidx.core:core-ktx:1.13.1")
}
