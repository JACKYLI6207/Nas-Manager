#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use base64::Engine;
    use image::{ImageBuffer, Rgba};
    use windows::core::{PWSTR, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
        ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        AssocQueryStringW, SHGetFileInfoW, ASSOCF_NONE, ASSOCSTR_EXECUTABLE, SHFILEINFOW,
        SHGFI_ICON, SHGFI_LARGEICON,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL};

    const ICON_SIZE: i32 = 32;

    pub fn icon_data_url_for_path(path: &str) -> Option<String> {
        let png = icon_png_for_file(path)?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        Some(format!("data:image/png;base64,{b64}"))
    }

    pub fn icon_data_url_for_system_default() -> Option<String> {
        if let Some(exe) = default_associated_exe(".mp4").or_else(|| default_associated_exe(".mkv"))
        {
            if let Some(url) = icon_data_url_for_path(&exe) {
                return Some(url);
            }
        }
        if let Ok(windir) = std::env::var("WINDIR") {
            for rel in [
                r"System32\wmplayer.exe",
                r"System32\ApplicationFrameHost.exe",
            ] {
                let p = Path::new(&windir).join(rel);
                if p.is_file() {
                    if let Some(url) = icon_data_url_for_path(&p.to_string_lossy()) {
                        return Some(url);
                    }
                }
            }
        }
        None
    }

    fn default_associated_exe(ext: &str) -> Option<String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let result = default_associated_exe_inner(ext);
            CoUninitialize();
            result
        }
    }

    unsafe fn default_associated_exe_inner(ext: &str) -> Option<String> {
        let wide: Vec<u16> = ext.encode_utf16().chain(std::iter::once(0)).collect();
        let mut buf = vec![0u16; 512];
        let mut len = buf.len() as u32;
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .ok()
        .ok()?;
        let exe = String::from_utf16_lossy(&buf[..len as usize])
            .trim_matches('\0')
            .to_string();
        if exe.is_empty() || !Path::new(&exe).exists() {
            None
        } else {
            Some(exe)
        }
    }

    fn icon_png_for_file(path: &str) -> Option<Vec<u8>> {
        unsafe {
            let wide: Vec<u16> = OsStr::new(path)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut sfi = SHFILEINFOW::default();
            let ok = SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                Default::default(),
                Some(&mut sfi as *mut _),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            );
            if ok == 0 {
                return None;
            }
            let rgba = hicon_to_rgba(sfi.hIcon, ICON_SIZE)?;
            let _ = DestroyIcon(sfi.hIcon);
            encode_png(&rgba, ICON_SIZE as u32)
        }
    }

    unsafe fn hicon_to_rgba(icon: windows::Win32::UI::WindowsAndMessaging::HICON, size: i32) -> Option<Vec<u8>> {
        let hdc_screen = GetDC(HWND::default());
        let hdc = CreateCompatibleDC(hdc_screen);
        let hbmp = CreateCompatibleBitmap(hdc_screen, size, size);
        let old = SelectObject(hdc, hbmp);
        let _ = DrawIconEx(hdc, 0, 0, icon, size, size, 0, None, DI_NORMAL);
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (size * size * 4) as usize];
        GetDIBits(
            hdc,
            hbmp,
            0,
            size as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        SelectObject(hdc, old);
        let _ = DeleteObject(hbmp);
        let _ = DeleteDC(hdc);
        let _ = ReleaseDC(HWND::default(), hdc_screen);

        // BGRA -> RGBA
        for chunk in buf.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        Some(buf)
    }

    fn encode_png(rgba: &[u8], size: u32) -> Option<Vec<u8>> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(size, size, rgba.to_vec())?;
        let mut out = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut out);
        img.write_to(
            &mut cursor,
            image::ImageFormat::Png,
        )
        .ok()?;
        Some(out)
    }
}

#[cfg(windows)]
pub use imp::{icon_data_url_for_path, icon_data_url_for_system_default};

#[cfg(not(windows))]
pub fn icon_data_url_for_path(_path: &str) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn icon_data_url_for_system_default() -> Option<String> {
    None
}
