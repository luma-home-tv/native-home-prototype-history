//! Small, scoped SDL/GLES window owner for the separate client. No system service calls.
use std::ffi::{c_char, c_int, c_void, CStr};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
extern "C" {
    fn SDL_Init(flags: u32) -> c_int;
    fn SDL_GL_SetAttribute(attr: c_int, value: c_int) -> c_int;
    fn SDL_CreateWindow(
        title: *const c_char,
        x: c_int,
        y: c_int,
        w: c_int,
        h: c_int,
        flags: u32,
    ) -> *mut c_void;
    fn SDL_GL_CreateContext(window: *mut c_void) -> *mut c_void;
    fn SDL_GL_GetDrawableSize(window: *mut c_void, w: *mut c_int, h: *mut c_int);
    fn SDL_GL_SetSwapInterval(interval: c_int) -> c_int;
    fn SDL_GL_SwapWindow(window: *mut c_void);
    fn SDL_GL_DeleteContext(ctx: *mut c_void);
    fn SDL_DestroyWindow(window: *mut c_void);
    fn SDL_Quit();
    fn SDL_GetError() -> *const c_char;
    fn glViewport(x: c_int, y: c_int, w: c_int, h: c_int);
    fn glReadPixels(
        x: c_int,
        y: c_int,
        w: c_int,
        h: c_int,
        format: u32,
        kind: u32,
        pixels: *mut c_void,
    );
    fn glGetString(name: u32) -> *const u8;
    fn glGetError() -> u32;
    fn native_home_poll(event: *mut Event) -> c_int;
}
#[derive(Default, Debug, serde::Serialize)]
#[repr(C)]
pub struct Event {
    pub kind: u32,
    pub key: u32,
    pub repeat: u32,
    pub source: u32,
    pub scan: i32,
    pub x: i32,
    pub y: i32,
    pub wheel: i32,
}
const _: () = assert!(std::mem::size_of::<Event>() == 32);
pub fn poll() -> Option<Event> {
    let mut event = Event::default();
    (unsafe { native_home_poll(&mut event) } != 0).then_some(event)
}
fn error() -> String {
    unsafe {
        CStr::from_ptr(SDL_GetError())
            .to_string_lossy()
            .into_owned()
    }
}
pub struct Window {
    window: *mut c_void,
    context: *mut c_void,
    pub renderer: String,
}
impl Window {
    pub fn new(visible: bool) -> Result<Self, Box<dyn std::error::Error>> {
        Self::create(visible, "com.ash.nativehome.preview")
    }
    pub fn launch_target() -> Result<Self, Box<dyn std::error::Error>> {
        Self::create(true, "com.ash.nativehome.targetproof")
    }
    fn create(visible: bool, expected_id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let device = cfg!(target_arch = "arm");
        if device && (unsafe { libc::geteuid() } == 0 || plx_base::paths::app_id() != expected_id) {
            return Err("Refusing root or unexpected package identity".into());
        }
        if device {
            // SDL_webOS.h's app-local Back opt-in, also used by upstream main.c. Home,
            // power and volume policies are deliberately left to LG.
            std::env::set_var("SDL_WEBOS_ACCESS_POLICY_KEYS_BACK", "true");
        }
        unsafe {
            let limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            libc::setrlimit(libc::RLIMIT_CORE, &limit);
            if SDL_Init(0x20) != 0 {
                return Err(error().into());
            }
            for (attr, value) in [
                (21, if device { 4 } else { 1 }),
                (17, if device { 2 } else { 4 }),
                (18, if device { 0 } else { 1 }),
            ] {
                if SDL_GL_SetAttribute(attr, value) != 0 {
                    SDL_Quit();
                    return Err(error().into());
                }
            }
            let window = SDL_CreateWindow(
                c"Luma Home Preview".as_ptr(),
                0,
                0,
                1920,
                1080,
                0x2 | if visible { 0x4 } else { 0x8 },
            );
            if window.is_null() {
                SDL_Quit();
                return Err(error().into());
            }
            let context = SDL_GL_CreateContext(window);
            if context.is_null() {
                SDL_DestroyWindow(window);
                SDL_Quit();
                return Err(error().into());
            }
            let mut owner = Self {
                window,
                context,
                renderer: String::new(),
            };
            let (mut w, mut h) = (0, 0);
            SDL_GL_GetDrawableSize(window, &mut w, &mut h);
            if (w, h) != (1920, 1080) {
                return Err(format!("Unexpected drawable {w}x{h}").into());
            }
            let ptr = glGetString(0x1f01);
            if ptr.is_null() {
                return Err("GL renderer unavailable".into());
            }
            owner.renderer = CStr::from_ptr(ptr.cast()).to_string_lossy().into_owned();
            glViewport(0, 0, w, h);
            SDL_GL_SetSwapInterval(1);
            plx_gfx::gfx::init_gl();
            plx_gfx::gfx::init_image();
            plx_gfx::text::init_text();
            Ok(owner)
        }
    }
    pub fn swap(&self) {
        unsafe {
            SDL_GL_SwapWindow(self.window);
        }
    }
    pub fn capture(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let mut rgba = vec![0_u8; 1920 * 1080 * 4];
        unsafe {
            glReadPixels(0, 0, 1920, 1080, 0x1908, 0x1401, rgba.as_mut_ptr().cast());
            let e = glGetError();
            if e != 0 {
                return Err(format!("Framebuffer error {e:#x}").into());
            }
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o640)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        write!(file, "P6\n1920 1080\n255\n")?;
        let mut row = vec![0_u8; 1920 * 3];
        for y in (0..1080).rev() {
            for x in 0..1920 {
                row[x * 3..x * 3 + 3]
                    .copy_from_slice(&rgba[(y * 1920 + x) * 4..(y * 1920 + x) * 4 + 3]);
            }
            file.write_all(&row)?;
        }
        Ok(())
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            SDL_GL_DeleteContext(self.context);
            SDL_DestroyWindow(self.window);
            SDL_Quit();
        }
    }
}
