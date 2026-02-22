{
  description = "RustySound development nix environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config = {
            android_sdk.accept_license = true;
            allowUnfree = true;
          };
        };
        androidPackages =
          if pkgs.stdenv.isLinux then
            pkgs.androidenv.composeAndroidPackages {
              platformVersions = [ "33" "34" ];
              buildToolsVersions = [ "33.0.2" "34.0.0" ];
              abiVersions = [ "x86_64" "arm64-v8a" ];
              cmakeVersions = [ "3.22.1" ];
              ndkVersions = [ "26.3.11579264" ];
              includeEmulator = true;
              includeSystemImages = true;
              systemImageTypes = [ "google_apis" ];
              includeNDK = true;
            }
          else
            null;
        rustysoundPkg = pkgs.callPackage ./packaging/nix/default.nix { };
      in
      {
        packages.rustysound = rustysoundPkg;
        packages.default = rustysoundPkg;
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustup
            pkg-config
            openssl
            unzip
            curl
            gcc
            clang
            just
            gnumake
            glib
            gtk3
            webkitgtk_4_1
            libsoup_3
            cairo
            pango
            gdk-pixbuf
            atk
            xdotool
            gst_all_1.gstreamer
            gst_all_1.gst-plugins-base
            gst_all_1.gst-plugins-good
            gst_all_1.gst-plugins-bad
            gst_all_1.gst-plugins-ugly
            gst_all_1.gst-libav
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            androidPackages.androidsdk
            android-tools
            jdk17
            gradle
          ];

          shellHook = ''
            export PATH="$HOME/.cargo/bin:$PATH"
            export OPENSSL_DIR="${pkgs.openssl.dev}"
            export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
            export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [
              pkgs.glib
              pkgs.gtk3
              pkgs.webkitgtk_4_1
              pkgs.libsoup_3
              pkgs.cairo
              pkgs.pango
              pkgs.gdk-pixbuf
              pkgs.atk
              pkgs.xdotool
              pkgs.gst_all_1.gstreamer
              pkgs.gst_all_1.gst-plugins-base
              pkgs.gst_all_1.gst-plugins-good
              pkgs.gst_all_1.gst-plugins-bad
              pkgs.gst_all_1.gst-plugins-ugly
              pkgs.gst_all_1.gst-libav
              pkgs.openssl
            ]}:$LD_LIBRARY_PATH"
            export GST_PLUGIN_SCANNER="${pkgs.gst_all_1.gstreamer}/libexec/gstreamer-1.0/gst-plugin-scanner"
            export GST_PLUGIN_SYSTEM_PATH_1_0="${pkgs.gst_all_1.gst-plugins-base}/lib/gstreamer-1.0:${pkgs.gst_all_1.gst-plugins-good}/lib/gstreamer-1.0:${pkgs.gst_all_1.gst-plugins-bad}/lib/gstreamer-1.0:${pkgs.gst_all_1.gst-plugins-ugly}/lib/gstreamer-1.0:${pkgs.gst_all_1.gst-libav}/lib/gstreamer-1.0:''${GST_PLUGIN_SYSTEM_PATH_1_0:-}"
            ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
              export ANDROID_HOME="${androidPackages.androidsdk}/libexec/android-sdk"
              export ANDROID_SDK_ROOT="$ANDROID_HOME"
              export ANDROID_NDK_ROOT="$ANDROID_HOME/ndk-bundle"
              export ANDROID_USER_HOME="$HOME/.android"
              export JAVA_HOME="${pkgs.jdk17.home}"
              export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
              export ANDROID_AAPT2_FROM_MAVEN_OVERRIDE="$ANDROID_HOME/build-tools/34.0.0/aapt2"
              export GRADLE_OPTS="-Dorg.gradle.project.android.aapt2FromMavenOverride=$ANDROID_AAPT2_FROM_MAVEN_OVERRIDE ''${GRADLE_OPTS:-}"
            ''}

            echo "RustySound dev shell loaded."
            echo "Run once to install CLI: cargo install dioxus-cli --locked"
            echo "Linux desktop dev: dx serve --platform desktop"
            echo "Web dev: dx serve --platform web"
            ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
              echo "Android dev: dx serve --platform android"
              echo "Android bundle: dx bundle --platform android --release"
              echo "ADB check: adb devices"
              echo "AAPT2 override: $ANDROID_AAPT2_FROM_MAVEN_OVERRIDE"
            ''}
          '';
        };
      }
    );
}
