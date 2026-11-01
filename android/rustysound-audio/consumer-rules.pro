# Rust calls these over JNI by name; keep them through release minification.
-keep class app.adarcher.rustysound.audio.AudioBridge { public static *; }
-keep class app.adarcher.rustysound.audio.PlaybackService
