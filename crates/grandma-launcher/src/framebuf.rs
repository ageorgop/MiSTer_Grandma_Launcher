// SPDX-License-Identifier: GPL-3.0-or-later

pub struct Framebuffer {
    width: u32,
    height: u32,
    stride: u32,
    bpp: u32,
    pub buffer: Vec<u8>,
    #[cfg(target_os = "linux")]
    mmap_ptr: *mut u8,
    #[cfg(target_os = "linux")]
    mmap_len: usize,
}

// SAFETY: The mmap pointer is only used from the main thread (no async/threads in launcher)
#[cfg(target_os = "linux")]
unsafe impl Send for Framebuffer {}

#[derive(Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };
    #[cfg(test)]
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const HIGHLIGHT: Self = Self { r: 80, g: 160, b: 255, a: 255 };
    pub const DARK_BG: Self = Self { r: 20, g: 20, b: 30, a: 255 };
}

#[cfg(target_os = "linux")]
const FBIOGET_VSCREENINFO: libc::Ioctl = 0x4600;
#[cfg(target_os = "linux")]
const FBIOGET_FSCREENINFO: libc::Ioctl = 0x4602;

/// Subset of fb_var_screeninfo (full struct is 160 bytes)
#[cfg(target_os = "linux")]
#[repr(C)]
struct FbVarScreeninfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    _pad: [u8; 128],
}

/// Subset of fb_fix_screeninfo
#[cfg(target_os = "linux")]
#[repr(C)]
struct FbFixScreeninfo {
    id: [u8; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    type_: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    _pad0: u16,
    line_length: u32,
    _pad: [u8; 32],
}

impl Framebuffer {
    /// Open the Linux framebuffer via /dev/fb0
    pub fn open(resolution: &grandma_common::config::Resolution) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            // Set resolution via vmode
            match std::process::Command::new("vmode")
                .arg("-r")
                .arg(resolution.width.to_string())
                .arg(resolution.height.to_string())
                .arg("rgb32")
                .status()
            {
                Ok(status) => {
                    if status.success() {
                        log::info!("vmode set to {}x{} rgb32", resolution.width, resolution.height);
                    } else {
                        log::warn!("vmode exited with status {} — continuing anyway", status);
                    }
                }
                Err(e) => log::warn!("Failed to run vmode: {} — continuing anyway", e),
            }

            // Open /dev/fb0
            let file = match std::fs::OpenOptions::new().read(true).write(true).open("/dev/fb0") {
                Ok(f) => f,
                Err(e) => {
                    log::warn!("Failed to open /dev/fb0: {} — using mock framebuffer", e);
                    return Ok(Self::mock(resolution.width, resolution.height));
                }
            };

            let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);

            // Query framebuffer dimensions via ioctl
            let mut vinfo: FbVarScreeninfo = unsafe { std::mem::zeroed() };
            let mut finfo: FbFixScreeninfo = unsafe { std::mem::zeroed() };

            if unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut vinfo) } < 0 {
                log::warn!("FBIOGET_VSCREENINFO failed: {} — using mock", std::io::Error::last_os_error());
                return Ok(Self::mock(resolution.width, resolution.height));
            }

            if unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut finfo) } < 0 {
                log::warn!("FBIOGET_FSCREENINFO failed: {} — using mock", std::io::Error::last_os_error());
                return Ok(Self::mock(resolution.width, resolution.height));
            }

            let width = vinfo.xres;
            let height = vinfo.yres;
            let bpp = (vinfo.bits_per_pixel / 8) as u32;
            let stride = finfo.line_length;
            let mmap_len = finfo.smem_len as usize;

            log::info!("fb0: {}x{} bpp={} stride={} size={}", width, height, bpp, stride, mmap_len);

            if mmap_len == 0 {
                log::warn!("fb0 reported smem_len=0 — using mock");
                return Ok(Self::mock(resolution.width, resolution.height));
            }

            if bpp != 4 {
                log::warn!("Expected 32bpp, got {}bpp — using mock", bpp * 8);
                return Ok(Self::mock(resolution.width, resolution.height));
            }

            let mmap_ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    mmap_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    0,
                )
            };

            if mmap_ptr == libc::MAP_FAILED {
                log::warn!("fb0 mmap failed: {} — using mock", std::io::Error::last_os_error());
                return Ok(Self::mock(resolution.width, resolution.height));
            }

            std::mem::forget(file);

            log::info!("Opened /dev/fb0: {}x{} stride={}", width, height, stride);

            Ok(Self {
                width,
                height,
                stride,
                bpp,
                buffer: vec![0u8; (stride * height) as usize],
                mmap_ptr: mmap_ptr as *mut u8,
                mmap_len,
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self::mock(resolution.width, resolution.height))
        }
    }

    /// Create a mock framebuffer for testing
    pub fn mock(width: u32, height: u32) -> Self {
        let bpp = 4;
        let stride = width * bpp;
        Self {
            width,
            height,
            stride,
            bpp,
            buffer: vec![0u8; (stride * height) as usize],
            #[cfg(target_os = "linux")]
            mmap_ptr: std::ptr::null_mut(),
            #[cfg(target_os = "linux")]
            mmap_len: 0,
        }
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }

    pub fn clear(&mut self, color: Color) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.set_pixel(x, y, color);
            }
        }
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height { return; }
        let offset = (y * self.stride + x * self.bpp) as usize;
        if offset + 3 < self.buffer.len() {
            // BGRA format (standard Linux framebuffer)
            self.buffer[offset] = color.b;
            self.buffer[offset + 1] = color.g;
            self.buffer[offset + 2] = color.r;
            self.buffer[offset + 3] = color.a;
        }
    }

    /// Blit a pre-decoded RGBA image at (x, y)
    pub fn blit_rgba(&mut self, x: u32, y: u32, img_width: u32, img_height: u32, rgba: &[u8]) {
        for iy in 0..img_height {
            for ix in 0..img_width {
                let src_offset = ((iy * img_width + ix) * 4) as usize;
                if src_offset + 3 >= rgba.len() { continue; }
                let color = Color {
                    r: rgba[src_offset],
                    g: rgba[src_offset + 1],
                    b: rgba[src_offset + 2],
                    a: rgba[src_offset + 3],
                };
                if color.a > 0 {
                    self.set_pixel(x + ix, y + iy, color);
                }
            }
        }
    }

    /// Fill a rectangle
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        for iy in y..y.saturating_add(h).min(self.height) {
            for ix in x..x.saturating_add(w).min(self.width) {
                self.set_pixel(ix, iy, color);
            }
        }
    }

    /// Dim a rectangular region by blending toward black
    pub fn dim_rect(&mut self, x: u32, y: u32, w: u32, h: u32, amount: u8) {
        let factor = (255 - amount) as u16;
        for iy in y..y.saturating_add(h).min(self.height) {
            for ix in x..x.saturating_add(w).min(self.width) {
                let offset = (iy * self.stride + ix * self.bpp) as usize;
                if offset + 3 < self.buffer.len() {
                    self.buffer[offset] = ((self.buffer[offset] as u16 * factor) / 255) as u8;
                    self.buffer[offset + 1] = ((self.buffer[offset + 1] as u16 * factor) / 255) as u8;
                    self.buffer[offset + 2] = ((self.buffer[offset + 2] as u16 * factor) / 255) as u8;
                }
            }
        }
    }

    /// Draw a rectangle border
    pub fn draw_rect(&mut self, x: u32, y: u32, w: u32, h: u32, thickness: u32, color: Color) {
        self.fill_rect(x, y, w, thickness, color);
        self.fill_rect(x, y + h - thickness, w, thickness, color);
        self.fill_rect(x, y, thickness, h, color);
        self.fill_rect(x + w - thickness, y, thickness, h, color);
    }

    /// Flush local buffer to the framebuffer device
    pub fn present(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if !self.mmap_ptr.is_null() && self.mmap_len > 0 {
                let len = self.buffer.len().min(self.mmap_len);
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.buffer.as_ptr(),
                        self.mmap_ptr,
                        len,
                    );
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for Framebuffer {
    fn drop(&mut self) {
        if !self.mmap_ptr.is_null() && self.mmap_len > 0 {
            unsafe {
                libc::munmap(self.mmap_ptr as *mut libc::c_void, self.mmap_len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_with_resolution() {
        let res = grandma_common::config::Resolution { width: 640, height: 480 };
        let fb = Framebuffer::open(&res).unwrap();
        assert_eq!(fb.width(), 640);
        assert_eq!(fb.height(), 480);
    }

    #[test]
    fn test_dim_rect() {
        let mut fb = Framebuffer::mock(100, 100);
        fb.fill_rect(0, 0, 100, 100, Color::WHITE);
        fb.dim_rect(0, 0, 100, 100, 128);
        let offset = (50 * 100 + 50) * 4;
        assert!(fb.buffer[offset + 2] < 200); // R channel is dimmed
        assert!(fb.buffer[offset + 2] > 100); // But not black
    }

    #[test]
    fn test_mock_framebuffer() {
        let mut fb = Framebuffer::mock(320, 240);
        fb.clear(Color::BLACK);
        fb.set_pixel(10, 10, Color::WHITE);
        assert_eq!(fb.width(), 320);
        assert_eq!(fb.height(), 240);
    }

    #[test]
    fn test_fill_rect() {
        let mut fb = Framebuffer::mock(100, 100);
        fb.fill_rect(10, 10, 50, 50, Color::WHITE);
        let offset = (20 * 100 + 20) * 4;
        assert_eq!(fb.buffer[offset + 2], 255); // R
    }
}
