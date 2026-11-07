# RustySound built from this checkout (used by flake.nix: `nix build`).
# For the prebuilt release binary, see AD-Archer/packages nix/pkgs/rustysound.
#
# The Linux desktop build inlines its CSS and serves assets/ from the folder
# next to the binary, so the real binary lives in lib/rustysound with assets/
# beside it and bin/rustysound is a wrapper that execs it.
{ lib
, rustPlatform
, pkg-config
, wrapGAppsHook3
, makeWrapper
, openssl
, webkitgtk_4_1
, gtk3
, libsoup_3
, glib
, glib-networking
, xdotool
, gst_all_1
}:

rustPlatform.buildRustPackage {
  pname = "rustysound";
  version = (lib.importTOML ../../Cargo.toml).package.version;

  src = lib.cleanSource ../..;
  cargoLock.lockFile = ../../Cargo.lock;

  buildNoDefaultFeatures = true;
  buildFeatures = [ "desktop" ];
  doCheck = false;

  nativeBuildInputs = [ pkg-config wrapGAppsHook3 makeWrapper ];

  buildInputs = [
    openssl
    webkitgtk_4_1
    gtk3
    libsoup_3
    glib
    glib-networking
    xdotool
    # WebKit plays audio through GStreamer.
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-libav
  ];

  # The wrapper is made by hand so it points at the real binary next to assets/.
  dontWrapGApps = true;

  postInstall = ''
    mkdir -p $out/lib/rustysound
    mv $out/bin/rustysound $out/lib/rustysound/rustysound
    cp -r assets $out/lib/rustysound/assets
    rm -f $out/lib/rustysound/assets/favicon.icns
    install -Dm644 packaging/linux/rustysound.desktop $out/share/applications/rustysound.desktop
    install -Dm644 assets/web-app-manifest-512x512.png $out/share/icons/hicolor/512x512/apps/rustysound.png
  '';

  preFixup = ''
    makeWrapper $out/lib/rustysound/rustysound $out/bin/rustysound \
      "''${gappsWrapperArgs[@]}"
  '';

  meta = {
    description = "Cross-platform music streaming client for Navidrome/Subsonic servers";
    homepage = "https://github.com/AD-Archer/RustySound";
    license = lib.licenses.gpl3Plus;
    platforms = lib.platforms.linux;
    mainProgram = "rustysound";
  };
}
