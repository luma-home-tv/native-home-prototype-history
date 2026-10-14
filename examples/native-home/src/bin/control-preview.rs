//! Native desktop framebuffer capture. No web renderer, Plex connection or television calls.
use std::ffi::{c_char, c_int, c_void, CStr};
use std::io::Write;

extern "C" {
    fn SDL_Init(flags: u32) -> c_int;
    fn SDL_GL_SetAttribute(attr: c_int, value: c_int) -> c_int;
    fn SDL_CreateWindow(title: *const c_char, x: c_int, y: c_int, w: c_int, h: c_int, flags: u32) -> *mut c_void;
    fn SDL_GL_CreateContext(window: *mut c_void) -> *mut c_void;
    fn SDL_GL_GetDrawableSize(window: *mut c_void, w: *mut c_int, h: *mut c_int);
    fn SDL_GL_SwapWindow(window: *mut c_void);
    fn SDL_GL_DeleteContext(ctx: *mut c_void);
    fn SDL_DestroyWindow(window: *mut c_void);
    fn SDL_Quit();
    fn SDL_GetError() -> *const c_char;
    fn glViewport(x: c_int, y: c_int, w: c_int, h: c_int);
    fn glReadPixels(x: c_int, y: c_int, w: c_int, h: c_int, format: u32, kind: u32, pixels: *mut c_void);
    fn glGetString(name: u32) -> *const u8;
    fn glGetError() -> u32;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("Specify a PPM output file")?;
    unsafe {
        if SDL_Init(0x20) != 0 { return Err(CStr::from_ptr(SDL_GetError()).to_string_lossy().into_owned().into()); }
        SDL_GL_SetAttribute(21, 1); // Desktop core profile; the TV build uses GLES2 separately.
        SDL_GL_SetAttribute(17, 4); SDL_GL_SetAttribute(18, 1);
        let window = SDL_CreateWindow(c"Native Home control preview".as_ptr(), 0, 0, 1920, 1080, 0x2 | 0x8);
        if window.is_null() { return Err(CStr::from_ptr(SDL_GetError()).to_string_lossy().into_owned().into()); }
        let ctx = SDL_GL_CreateContext(window);
        if ctx.is_null() { return Err(CStr::from_ptr(SDL_GetError()).to_string_lossy().into_owned().into()); }
        let (mut w, mut h) = (0, 0); SDL_GL_GetDrawableSize(window, &mut w, &mut h);
        if (w, h) != (1920, 1080) { return Err(format!("Unexpected drawable {w}x{h}").into()); }
        println!("Host capture renderer: {}", CStr::from_ptr(glGetString(0x1f01).cast()).to_string_lossy());
        glViewport(0, 0, w, h);
        plx_gfx::gfx::init_gl(); plx_gfx::gfx::init_image(); plx_gfx::text::init_text();
        let mut page = native_home::preview::ControlPreview::new();
        for _ in 0..45 { page.draw(1.0 / 60.0); SDL_GL_SwapWindow(window); }
        page.draw(1.0 / 60.0);
        let mut rgba = vec![0_u8; w as usize * h as usize * 4];
        glReadPixels(0, 0, w, h, 0x1908, 0x1401, rgba.as_mut_ptr().cast());
        let error = glGetError(); if error != 0 { return Err(format!("Framebuffer GL error {error:#x}").into()); }
        let mut file = std::fs::File::create(&output)?;
        write!(file, "P6\n{w} {h}\n255\n")?;
        let mut row = vec![0_u8; w as usize * 3];
        for y in (0..h as usize).rev() {
            for x in 0..w as usize { row[x*3..x*3+3].copy_from_slice(&rgba[(y*w as usize+x)*4..(y*w as usize+x)*4+3]); }
            file.write_all(&row)?;
        }
        SDL_GL_DeleteContext(ctx); SDL_DestroyWindow(window); SDL_Quit();
        println!("Captured {w}x{h} to {output}; host visual evidence only");
    }
    Ok(())
}
