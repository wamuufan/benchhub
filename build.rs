fn main() {
    println!("cargo:rerun-if-changed=ui/appwindow.slint");
    println!("cargo:rerun-if-changed=src/ssl_shim.c");

    slint_build::compile("ui/appwindow.slint").unwrap();

    // Compile Linux SSL redirection shim for cross-distro CA certificate compatibility using cc crate
    let compiler = cc::Build::new().opt_level(2).pic(true).get_compiler();

    let mut cmd = compiler.to_command();
    cmd.args(["-shared", "src/ssl_shim.c", "-o", "libssl_shim.so", "-ldl"]);

    let status = cmd
        .status()
        .expect("Failed to execute C compiler for libssl_shim.so");

    if !status.success() {
        panic!(
            "Failed to compile libssl_shim.so: C compiler exited with code {:?}",
            status.code()
        );
    }
}
