fn main() {
    #[cfg(target_os = "macos")]
    println!("cargo::rustc-link-arg=-Wl,-rpath,@loader_path");

    #[cfg(target_os = "linux")]
    println!("cargo::rustc-link-arg=-Wl,-rpath,$ORIGIN");

    #[cfg(target_arch = "x86_64")]
    println!("cargo::rustc-cfg=target_feature=\"+sse2\"");
}
