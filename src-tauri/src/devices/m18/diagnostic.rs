use crate::storage;
use anyhow::Result;
use std::path::Path;
pub fn brightness_page(root: &Path) -> Result<crate::model::Page> {
    use image::{Rgb, RgbImage};
    let dir = root.join("cache");
    std::fs::create_dir_all(&dir)?;
    let mut page = super::m18::new_page("屏幕亮度测试");
    page.id = "builtin-brightness-test-v1".into();
    let solid = [
        [255, 255, 255],
        [192, 192, 192],
        [128, 128, 128],
        [64, 64, 64],
        [16, 16, 16],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 0],
        [0, 255, 255],
    ];
    for k in page.keys.iter_mut().filter(|k| k.key <= 15) {
        let index = usize::from(k.key - 1);
        let img = RgbImage::from_fn(64, 64, |x, y| {
            let v = (x * 255 / 63) as u8;
            Rgb(match index {
                0..=9 => solid[index],
                10 => [v, v, v],
                11 => match y / 16 {
                    0 => [v, 0, 0],
                    1 => [0, v, 0],
                    2 => [0, 0, v],
                    _ => [v, v, v],
                },
                12 => {
                    if (x / 8 + y / 8) % 2 == 0 {
                        [255; 3]
                    } else {
                        [0; 3]
                    }
                }
                13 => [((x / 8) * 4) as u8; 3],
                _ => [224 + ((x / 8) * 4) as u8; 3],
            })
        });
        let file = dir.join(format!("brightness-test-{}.png", k.key));
        img.save(&file)?;
        k.asset_id = Some(storage::import(root, &file)?);
        k.text.clear();
    }
    let bg = dir.join("brightness-test-background.png");
    RgbImage::from_fn(480, 272, |x, y| {
        let v = (x * 255 / 479) as u8;
        if y < 200 {
            Rgb([v; 3])
        } else {
            Rgb(solid[((x / 96) + 5) as usize])
        }
    })
    .save(&bg)?;
    page.background_asset = Some(storage::import(root, &bg)?);
    Ok(page)
}
