//! Shell icon extraction runs on the application's blocking pool, never on the
//! UI or device queue. Cached PNGs use the normal content-addressed asset store.
use anyhow::{bail, Result};
use std::path::Path;

pub fn cache(root: &Path, source: &str) -> Result<String> {
    crate::launcher::validate(source, "", None)?;
    if crate::launcher::windows_app_id(source).is_none() && !Path::new(source).is_file() {
        bail!(alchemy_device_api::UiFailure::new("backend.b013", "程序或快捷方式不存在，无法提取图标"))
    }
    #[cfg(windows)]
    {
        crate::storage::put_asset(
            root,
            &platform::extract(source, crate::launcher::windows_app_id(source).is_some())?,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        bail!(alchemy_device_api::UiFailure::new("backend.b014", "应用图标提取仅支持 Windows"))
    }
}

pub fn cache_registered(root: &Path, source: &str) -> Result<String> {
    #[cfg(windows)]
    {
        crate::storage::put_asset(root, &platform::extract(source, true)?)
    }
    #[cfg(not(windows))]
    {
        let _ = (root, source);
        bail!(alchemy_device_api::UiFailure::new("backend.b015", "应用图标仅支持 Windows"))
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use anyhow::{ensure, Context};
    use windows::{
        core::{Interface, PCWSTR},
        Win32::{
            Foundation::SIZE,
            Graphics::{
                Gdi::{DeleteObject, HBITMAP, HPALETTE},
                Imaging::*,
            },
            Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
            System::Com::*,
            UI::{
                Controls::{IImageList, ILD_TRANSPARENT},
                Shell::*,
                WindowsAndMessaging::{DestroyIcon, HICON},
            },
        },
    };
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    struct Icon(HICON);
    impl Drop for Icon {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyIcon(self.0);
            }
        }
    }
    struct Bitmap(HBITMAP);
    impl Drop for Bitmap {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteObject(self.0.into());
            }
        }
    }

    pub(super) fn extract(source: &str, registered: bool) -> Result<Vec<u8>> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        let _com = Com;
        let path: Vec<u16> = source.encode_utf16().chain(Some(0)).collect();
        let factory: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };
        let bitmap = if registered {
            let item: IShellItem = if crate::launcher::windows_app_id(source).is_some() {
                unsafe { SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None)? }
            } else {
                crate::launcher::platform::apps_folder_item(source)?
            };
            let images: IShellItemImageFactory = item.cast()?;
            let handle = Bitmap(unsafe {
                images.GetImage(
                    SIZE { cx: 256, cy: 256 },
                    SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
                )?
            });
            unsafe {
                factory.CreateBitmapFromHBITMAP(
                    handle.0,
                    HPALETTE::default(),
                    WICBitmapUsePremultipliedAlpha,
                )?
            }
        } else {
            let mut info = SHFILEINFOW::default();
            ensure!(
                unsafe {
                    SHGetFileInfoW(
                        PCWSTR(path.as_ptr()),
                        FILE_FLAGS_AND_ATTRIBUTES(0),
                        Some(&mut info),
                        std::mem::size_of::<SHFILEINFOW>() as u32,
                        SHGFI_SYSICONINDEX,
                    )
                } != 0, alchemy_device_api::UiFailure::new("backend.e007", "Windows 未能读取文件图标"));
            // Shell indexing honors the .lnk's custom icon, with its target as fallback.
            // Use HICON + WIC instead of guessing the alpha encoding of Shell HBITMAPs.
            let list: IImageList = unsafe { SHGetImageList(SHIL_JUMBO as i32)? };
            let icon = Icon(unsafe { list.GetIcon(info.iIcon, ILD_TRANSPARENT.0)? });
            unsafe { factory.CreateBitmapFromHICON(icon.0)? }
        };
        let mut width = 0;
        let mut height = 0;
        unsafe {
            bitmap.GetSize(&mut width, &mut height)?;
        }
        ensure!(
            (1..=1024).contains(&width) && (1..=1024).contains(&height), alchemy_device_api::UiFailure::new("backend.e008", "图标尺寸无效"));
        let converter = unsafe { factory.CreateFormatConverter()? };
        unsafe {
            converter.Initialize(
                &bitmap,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )?;
        }
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        unsafe {
            converter.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
        }
        let image =
            image::RgbaImage::from_raw(width, height, pixels).context(alchemy_device_api::UiFailure::new("backend.b016", "图标像素格式无效"))?;
        // Some legacy programs occupy only a small corner of the jumbo image.
        // Center the visible content in a padded square for consistent deck icons.
        let mut left = width;
        let mut top = height;
        let mut right = 0;
        let mut bottom = 0;
        for (x, y, p) in image.enumerate_pixels() {
            if p[3] > 0 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
        ensure!(left < width && top < height, alchemy_device_api::UiFailure::new("backend.e009", "应用图标为空"));
        let visible =
            image::imageops::crop_imm(&image, left, top, right - left + 1, bottom - top + 1)
                .to_image();
        // Resize in premultiplied space so transparent padding cannot introduce
        // dark fringes. WIC has already converted the source to straight RGBA.
        let mut visible = visible;
        for p in visible.pixels_mut() {
            let a = u16::from(p[3]);
            for c in &mut p.0[..3] {
                *c = ((u16::from(*c) * a + 127) / 255) as u8;
            }
        }
        let mut scaled = image::DynamicImage::ImageRgba8(visible)
            .resize(224, 224, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        for p in scaled.pixels_mut() {
            let a = u32::from(p[3]);
            if a > 0 {
                for c in &mut p.0[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            } else {
                p.0[..3].fill(0);
            }
        }
        let mut canvas = image::RgbaImage::new(256, 256);
        image::imageops::overlay(
            &mut canvas,
            &scaled,
            i64::from((256 - scaled.width()) / 2),
            i64::from((256 - scaled.height()) / 2),
        );
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(canvas).write_to(&mut png, image::ImageFormat::Png)?;
        Ok(png.into_inner())
    }
}
