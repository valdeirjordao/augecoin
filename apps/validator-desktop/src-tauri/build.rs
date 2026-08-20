fn main() {
    println!("cargo:rerun-if-env-changed=AUGECOIN_RELEASE_PUBLIC_KEY_HEX");
    tauri_build::build()
}
