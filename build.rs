use std::path::{Path, PathBuf};

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "android" {
        install_android_audio_bridge();
    }

    if target_os == "windows" {
        compile_windows_resources();
    }
}

fn compile_windows_resources() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/favicon.ico");
        res.set("ProductName", "RustySound");
        res.set("FileDescription", "RustySound");
        res.set("InternalName", "RustySound");
        res.set("OriginalFilename", "rustysound.exe");
        res.set("CompanyName", "AD-Archer");
        res.set("LegalCopyright", "Copyright (c) 2026 AD-Archer");
        if let Err(err) = res.compile() {
            panic!("failed to compile Windows resources: {err}");
        }
    }
}

fn install_android_audio_bridge() {
    println!("cargo:rerun-if-env-changed=WRY_ANDROID_KOTLIN_FILES_OUT_DIR");
    println!("cargo:rerun-if-env-changed=WRY_ANDROID_PACKAGE");
    println!("cargo:rerun-if-changed=android/kotlin/RustySoundAudioBridge.kt");

    let Ok(out_dir) = std::env::var("WRY_ANDROID_KOTLIN_FILES_OUT_DIR") else {
        return;
    };
    let Ok(package) = std::env::var("WRY_ANDROID_PACKAGE") else {
        return;
    };

    let out_dir = PathBuf::from(out_dir);
    if let Err(err) = std::fs::create_dir_all(&out_dir) {
        panic!("failed to create Android Kotlin output directory: {err}");
    }

    let template = std::fs::read_to_string("android/kotlin/RustySoundAudioBridge.kt")
        .expect("failed to read Android audio bridge Kotlin template");
    let rendered = format!(
        "/* THIS FILE IS AUTO-GENERATED. DO NOT MODIFY!! */\n\n{}",
        template.replace("{{package}}", &package)
    );
    let target = out_dir.join("RustySoundAudioBridge.kt");
    write_if_changed(&target, &rendered);

    if let Some(main_dir) = find_android_main_dir(&out_dir) {
        patch_android_manifest(&main_dir.join("AndroidManifest.xml"), &package);
    }
}

fn find_android_main_dir(kotlin_out_dir: &Path) -> Option<PathBuf> {
    for candidate in kotlin_out_dir.ancestors() {
        if candidate.file_name().and_then(|name| name.to_str()) == Some("main") {
            let manifest = candidate.join("AndroidManifest.xml");
            if manifest.exists() {
                return Some(candidate.to_path_buf());
            }
        }
    }
    None
}

fn patch_android_manifest(manifest_path: &Path, package: &str) {
    let Ok(mut manifest) = std::fs::read_to_string(manifest_path) else {
        return;
    };

    let permissions = [
        r#"<uses-permission android:name="android.permission.FOREGROUND_SERVICE" />"#,
        r#"<uses-permission android:name="android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK" />"#,
        r#"<uses-permission android:name="android.permission.WAKE_LOCK" />"#,
        r#"<uses-permission android:name="android.permission.POST_NOTIFICATIONS" />"#,
    ];
    for permission in permissions {
        if !manifest.contains(permission) {
            if let Some(index) = manifest.find("<application") {
                manifest.insert_str(index, &format!("    {permission}\n\n"));
            }
        }
    }

    let service_marker = "RustySoundAudioService";
    if !manifest.contains(service_marker) {
        let service = format!(
            r#"
        <service
            android:name="{package}.RustySoundAudioService"
            android:exported="false"
            android:foregroundServiceType="mediaPlayback" />
"#
        );
        if let Some(index) = manifest.rfind("</application>") {
            manifest.insert_str(index, &service);
        }
    }

    write_if_changed(manifest_path, &manifest);
}

fn write_if_changed(path: &Path, contents: &str) {
    let should_write = std::fs::read_to_string(path)
        .map(|existing| existing != contents)
        .unwrap_or(true);
    if should_write {
        std::fs::write(path, contents)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}
