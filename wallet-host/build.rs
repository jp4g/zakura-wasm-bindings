fn main() {
    if std::env::var_os("CARGO_FEATURE_WALLET_STORAGE").is_none()
        || std::env::var("TARGET").as_deref() != Ok("wasm32-unknown-unknown") { return; }
    let sdk = std::env::var("WALLET_SDK").expect("use build-wallet.py");
    let sqlite = std::env::var("WALLET_SQLITE").expect("use build-wallet.py");
    let out = std::env::var("OUT_DIR").unwrap();
    let object = format!("{out}/wallet-vfs.o");
    let status = std::process::Command::new(format!("{sdk}/bin/clang"))
        .args(["--target=wasm32-wasi", "-O2", "-ffunction-sections", "-fdata-sections",
            "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter", "-I", &sqlite,
            "-c", "wallet-host/adapter.c", "-o", &object]).status().unwrap();
    assert!(status.success(), "VFS compile failed");
    println!("cargo:rerun-if-changed=wallet-host/adapter.c");
    println!("cargo:rustc-link-arg={object}");
    println!("cargo:rustc-link-search=native={sdk}/share/wasi-sysroot/lib/wasm32-wasi");
    println!("cargo:rustc-link-lib=static=c");
    for symbol in ["wallet_runtime_init", "wallet_pool_start", "wallet_pool_size", "__heap_base"] {
        println!("cargo:rustc-link-arg=--export={symbol}");
    }
}
