fn main() {
    tauri_build::build();
    // Android 15+ / Pixel-16KB-Pages: die cdylib muss 16KB-aligned gelinkt
    // werden (LOAD-Segmente max-page-size=16384), sonst meckert die
    // LOAD-Segment-Pruefung des Systems. build.rs-Link-Args werden AN die
    // von der tauri-CLI gesetzten RUSTFLAGS angehaengt (env waere sonst
    // nicht erweiterbar).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
    }
}
