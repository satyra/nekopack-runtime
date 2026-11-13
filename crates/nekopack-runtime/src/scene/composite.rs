//! Resource syntax shared by full-screen visual composition commands.
use crate::dat::DatGame;
use nekopack_image::{decode_png, Metadata, Mng, RgbaImage};
use std::{
    error::Error,
    io::{Read, Seek},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(crate) fn read_visual<R: Read + Seek>(
    store: &mut DatGame<R>,
    name: &str,
    pixel_limit: usize,
) -> Result<RgbaImage> {
    let Some(composition) = name.strip_prefix('#') else {
        let bytes = store.read("image/visual", &format!("{name}.png"))?;
        if crate::story::png_position(&bytes)? != [0, 0] {
            return Err("unsupported visual PNG position".into());
        }
        return Ok(decode_png(&bytes, pixel_limit)?);
    };
    let (container, parts) = composition
        .split_once('&')
        .ok_or("visual composition requires named parts")?;
    let bytes = store.read("image/visual", &format!("{container}.mng"))?;
    compose(&bytes, parts, pixel_limit)
}

fn compose(bytes: &[u8], parts: &str, pixel_limit: usize) -> Result<RgbaImage> {
    let mng = Mng::parse(bytes, 100000)?;
    let sprites = mng.sprites()?;
    let mut canvas = RgbaImage::transparent(mng.header[0], mng.header[1], pixel_limit)?;
    let delta = mng.chunks.iter().any(|c| {
        matches!(
            c.metadata,
            Metadata::Termination {
                repeat: Some((_, _, 9999)),
                ..
            }
        )
    });
    let base = if delta {
        Some(mng.decode_image(0, pixel_limit)?)
    } else {
        None
    };
    for name in parts.split('&') {
        let mut selected = None;
        for sprite in &sprites {
            if crate::dat::decode(sprite.name)? == name {
                if selected.is_some() {
                    return Err("ambiguous visual composition part".into());
                }
                selected = Some(sprite);
            }
        }
        let sprite = selected.ok_or_else(|| format!("missing visual composition part {name}"))?;
        let mut pixels = mng.decode_image(sprite.ordinal, pixel_limit)?;
        if let Some(base) = base.as_ref().filter(|_| sprite.ordinal != 0) {
            if (pixels.width(), pixels.height()) != (base.width(), base.height()) {
                return Err("visual delta frame dimensions differ".into());
            }
            for (sample, reference) in pixels.pixels_mut().iter_mut().zip(base.pixels()) {
                *sample = sample.wrapping_add(*reference);
            }
        }
        canvas.draw(&pixels, sprite.position);
    }
    Ok(canvas)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_visual_parts_compose_in_order_and_survive_scene_checkpoint() {
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(kind);
            out.extend(body);
            let mut crc = !0u32;
            for byte in kind.iter().chain(body) {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
                }
            }
            out.extend((!crc).to_be_bytes());
        }
        let mut mng = b"\x8aMNG\r\n\x1a\n".to_vec();
        chunk(
            &mut mng,
            b"MHDR",
            &[1u32, 1, 0, 0, 0, 0, 0]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect::<Vec<_>>(),
        );
        for (label, color) in [("0000", [200, 0, 0, 255]), ("0001", [0, 100, 0, 128])] {
            let mut frame = vec![2];
            frame.extend(label.as_bytes());
            frame.extend([0, 0, 0, 0, 0]);
            chunk(&mut mng, b"FRAM", &frame);
            let mut png = Vec::new();
            RgbaImage::filled(1, 1, color, 4)
                .unwrap()
                .write_png(&mut png)
                .unwrap();
            mng.extend(&png[8..]);
        }
        chunk(&mut mng, b"MEND", &[]);
        let name = "#Synthetic&0000&0001";
        let length = mng.len() as u32;
        let mut pack = b"NEKOPACK".to_vec();
        for word in [
            1u32,
            0,
            0,
            16,
            nekopack_archive::name_hash(1, b"image/visual"),
            1,
            nekopack_archive::name_hash(1, b"Synthetic.mng"),
            length,
            0,
            length,
        ] {
            pack.extend(word.to_le_bytes());
        }
        pack.extend(mng);
        let mut store = DatGame::mount([std::io::Cursor::new(pack)]).unwrap();
        let image = read_visual(&mut store, name, 4).unwrap();
        assert_eq!(image.pixels(), &[100, 50, 0, 255]);
        assert!(read_visual(&mut store, "#Synthetic&missing", 4).is_err());
        let transparent = read_visual(&mut store, "#Synthetic&0001", 4).unwrap();
        assert_eq!(transparent.pixels(), &[0, 100, 0, 128]);
        let mut scene = crate::scene::Scene::new([1, 1]);
        scene.set_visual(name.into(), image).unwrap();
        let mut writer = nekopack_script::checkpoint::Writer::default();
        scene.save(&mut writer).unwrap();
        let mut reader = nekopack_script::checkpoint::Reader::new(&writer.0).unwrap();
        let restored = crate::scene::Scene::load(&mut reader, [1, 1]).unwrap();
        reader.finish().unwrap();
        let (restored_name, restored_image) = restored.visual.unwrap();
        assert_eq!(restored_name, name);
        assert_eq!(restored_image.pixels(), &[100, 50, 0, 255]);
    }
}
