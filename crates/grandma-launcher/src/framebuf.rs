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

/// FPGA framebuffer base address in DDR3 memory
#[cfg(target_os = "linux")]
const FPGA_FB_BASE: u64 = 0x22000000;
/// MiSTer skips the first 4096 bytes of the framebuffer region
#[cfg(target_os = "linux")]
const FPGA_FB_OFFSET: u64 = 4096;

/// Framebuffer parameters read from sysfs or defaults
#[derive(Debug, Clone)]
pub struct FbParams {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

impl FbParams {
    /// Default MiSTer framebuffer parameters (1080p BGRA)
    fn defaults() -> Self {
        Self { width: 1920, height: 1080, stride: 1920 * 4, format: 6 }
    }

    /// Read framebuffer parameters from MiSTer sysfs
    #[cfg(target_os = "linux")]
    pub fn from_sysfs() -> Self {
        let read = |name: &str| -> Option<String> {
            let path = format!("/sys/module/MiSTer_fb/parameters/{}", name);
            std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
        };
        Self::parse_from_strings(
            read("width").as_deref(),
            read("height").as_deref(),
            read("stride").as_deref(),
            read("format").as_deref(),
        )
    }

    /// Parse parameters from optional strings, falling back to defaults
    pub fn parse_from_strings(
        width: Option<&str>,
        height: Option<&str>,
        stride: Option<&str>,
        format: Option<&str>,
    ) -> Self {
        let defaults = Self::defaults();
        let w = width.and_then(|s| s.parse().ok()).unwrap_or(defaults.width);
        let h = height.and_then(|s| s.parse().ok()).unwrap_or(defaults.height);
        let mut s = stride.and_then(|s| s.parse().ok()).unwrap_or(w * 4);
        let f = format.and_then(|s| s.parse().ok()).unwrap_or(defaults.format);
        if s < w * 4 {
            s = w * 4;
        }
        Self { width: w, height: h, stride: s, format: f }
    }
}

impl Framebuffer {
    /// Open the MiSTer FPGA framebuffer via /dev/mem mmap
    pub fn open() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            let params = FbParams::from_sysfs();
            log::info!(
                "FB params: {}x{} stride={} format={}",
                params.width, params.height, params.stride, params.format
            );

            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/mem")
                .map_err(|e| format!("Failed to open /dev/mem: {}", e))?;

            let bpp = 4u32;
            let mmap_len = (params.stride * params.height * 3) as usize;
            let mmap_offset = FPGA_FB_BASE + FPGA_FB_OFFSET;

            let mmap_ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    mmap_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    std::os::unix::io::AsRawFd::as_raw_fd(&file),
                    mmap_offset as libc::off_t,
                )
            };

            if mmap_ptr == libc::MAP_FAILED {
                log::warn!(
                    "FPGA framebuffer mmap failed: {} — using mock for development",
                    std::io::Error::last_os_error()
                );
                return Ok(Self::mock(params.width, params.height));
            }

            std::mem::forget(file);

            log::info!(
                "Opened FPGA framebuffer: {}x{} stride={} at 0x{:x}+{}",
                params.width, params.height, params.stride, FPGA_FB_BASE, FPGA_FB_OFFSET
            );

            Ok(Self {
                width: params.width,
                height: params.height,
                stride: params.stride,
                bpp,
                buffer: vec![0u8; mmap_len],
                mmap_ptr: mmap_ptr as *mut u8,
                mmap_len,
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err("Framebuffer not available on this platform".to_string())
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
    fn test_parse_sysfs_params_valid() {
        let params = FbParams::parse_from_strings(
            Some("1920"), Some("1080"), Some("7680"), Some("6"),
        );
        assert_eq!(params.width, 1920);
        assert_eq!(params.height, 1080);
        assert_eq!(params.stride, 7680);
        assert_eq!(params.format, 6);
    }

    #[test]
    fn test_parse_sysfs_params_missing_falls_back() {
        let params = FbParams::parse_from_strings(None, None, None, None);
        assert_eq!(params.width, 1920);
        assert_eq!(params.height, 1080);
        assert_eq!(params.stride, 7680);
    }

    #[test]
    fn test_parse_sysfs_params_partial() {
        let params = FbParams::parse_from_strings(
            Some("1280"), Some("720"), None, None,
        );
        assert_eq!(params.width, 1280);
        assert_eq!(params.height, 720);
        assert_eq!(params.stride, 1280 * 4);
    }

    #[test]
    fn test_parse_sysfs_params_corrupt_stride() {
        let params = FbParams::parse_from_strings(
            Some("1920"), Some("1080"), Some("100"), Some("6"),
        );
        assert_eq!(params.stride, 1920 * 4);
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
