//! Isolated launch target: a native surface, bounded lifetime, no TV controls or account access.
use native_home::runtime::{poll, Window};
use plx_ui::{label::Label, theme, Painter, Rect};
use std::{
    os::unix::fs::OpenOptionsExt,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = Window::launch_target()?;
    let info = serde_json::json!({"app_id":plx_base::paths::app_id(),"uid":unsafe{libc::geteuid()},"pid":std::process::id(),"renderer":window.renderer,"setting_writes":false,"account_access":false});
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(plx_base::paths::in_runtime_dir(&format!(
            "target-{}.json",
            std::process::id()
        )))?;
    serde_json::to_writer_pretty(&mut file, &info)?;
    let start = Instant::now();
    let mut stop = false;
    let mut frames=0;
    while start.elapsed() < Duration::from_secs(12) && !stop {
        let p = Painter::root();
        let ground = theme::SURFACE_APP;
        plx_gfx::gfx::frame_clear(ground[0], ground[1], ground[2]);
        Label::new(
            c"Native launch target".as_ptr(),
            theme::size::DISPLAY,
            theme::TEXT_PRIMARY,
        )
        .bold()
        .draw(p, Rect::new(96.0, 100.0, 1728.0, 80.0));
        Label::new(
            c"Isolated application launch proof".as_ptr(),
            theme::size::TITLE,
            theme::TEXT_SECONDARY,
        )
        .draw(p, Rect::new(96.0, 240.0, 1728.0, 80.0));
        Label::new(
            c"No settings or viewing accounts are accessed. Back exits.".as_ptr(),
            theme::size::BODY,
            theme::TEXT_SECONDARY,
        )
        .draw(p, Rect::new(96.0, 340.0, 1728.0, 80.0));
        frames+=1;
        if frames==45{window.capture(&plx_base::paths::in_runtime_dir(&format!("target-{}.ppm",std::process::id())))?;}
        window.swap();
        while let Some(event) = poll() {
            if event.kind == 1 || (event.kind == 2 && event.key == 6) {
                stop = true;
            }
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}
