fn main() {
    println!("cargo:rerun-if-changed=ui/appwindow.slint");

    let mut config = slint_build::CompilerConfiguration::new();
    config = config.with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/appwindow.slint", config).unwrap();
}
