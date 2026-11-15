#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn configure_linux_webkit_env() {
    // WebKitGTK's DMA-BUF renderer can flash dark/black frames during rapid hover or repaint
    // updates when the NVIDIA proprietary driver is loaded; fall back cleanly unless overridden.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
        && (std::path::Path::new("/sys/module/nvidia").exists()
            || std::path::Path::new("/dev/nvidiactl").exists())
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    configure_linux_webkit_env();

    benchhub_desktop::run();
}
