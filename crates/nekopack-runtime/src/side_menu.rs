//! shortcut.nut's SIDEMENU order and SLOT-relative sprite geometry.
use crate::{
    dat::DatGame,
    quote,
    title::{contains, update_pointer},
};
use nekopack_image::{Mng, RgbaImage};
use std::{
    collections::BTreeMap,
    error::Error,
    io::{Read, Seek},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BUDGET: usize = 32 * 1024 * 1024;
const ACTIONS: [&str; 8] = [
    "QSAVE", "QLOAD", "SAVE", "LOAD", "AUTO", "SKIP", "LOG", "CONFIG",
];
struct Sprite {
    image: RgbaImage,
    position: [i32; 2],
}

pub(crate) struct SideMenu {
    images: BTreeMap<String, Sprite>,
    size: [u32; 2],
    quick: bool,
    popup: bool,
    pressed: Option<i32>,
    hover: Option<i32>,
    sequence: u64,
}
impl SideMenu {
    pub fn open<R: Read + Seek>(store: &mut DatGame<R>, quick: bool, popup: bool) -> Result<Self> {
        let bytes = store.read("system", "shortcut.mng")?;
        let mng = Mng::parse(&bytes, 100000)?;
        let size = [mng.header[0], mng.header[1]];
        let mut retained = RgbaImage::transparent(size[0], size[1], BUDGET)?
            .pixels()
            .len();
        let mut images = BTreeMap::new();
        for sprite in mng.sprites()? {
            let name = crate::dat::decode(sprite.name)?
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let image = mng.decode_image(sprite.ordinal, BUDGET - retained)?;
            retained += image.pixels().len();
            images.insert(
                name,
                Sprite {
                    image,
                    position: sprite.position,
                },
            );
        }
        for name in ["* BASE".to_string(), "- QLOAD".into()].into_iter().chain(
            ACTIONS
                .iter()
                .enumerate()
                .flat_map(|(i, a)| [format!("= {a}"), format!("SLOT{i}"), a.to_string()]),
        ) {
            if !images.contains_key(&name) {
                return Err(format!("missing side menu sprite {name}").into());
            }
        }
        Ok(Self {
            images,
            size,
            quick,
            popup,
            pressed: None,
            hover: None,
            sequence: 0,
        })
    }
    fn enabled(&self, i: usize) -> bool {
        i != 1 || self.quick
    }
    fn bounds(&self, i: usize) -> [i32; 4] {
        let s = &self.images[&format!("SLOT{i}")];
        [
            s.position[0],
            s.position[1],
            s.image.width() as i32,
            s.image.height() as i32,
        ]
    }
    pub fn pointer(&mut self, kind: i32, x: i32, y: i32) -> Result<Vec<u8>> {
        let hit = (0..8)
            .find(|i| {
                let b = self.bounds(*i);
                self.enabled(*i) && contains([b[0], b[1]], [b[2] as u32, b[3] as u32], [x, y])
            })
            .map(|i| i as i32);
        self.hover = if kind == 3 { None } else { hit };
        let action = if kind == 4 {
            None
        } else {
            update_pointer(&mut self.pressed, kind, hit)?
        };
        self.sequence += 1;
        self.packet(action.map(|i| ACTIONS[i as usize]).unwrap_or(""))
    }
    pub fn packet(&self, action: &str) -> Result<Vec<u8>> {
        let mut image = RgbaImage::transparent(self.size[0], self.size[1], BUDGET)?;
        let mut buttons = Vec::new();
        for (i, name) in ACTIONS.iter().enumerate() {
            let b = self.bounds(i);
            let enabled = self.enabled(i);
            let sprite = &self.images[&format!("{} {name}", if enabled { "=" } else { "-" })];
            image.draw(
                &sprite.image,
                [b[0] + sprite.position[0], b[1] + sprite.position[1]],
            );
            buttons.push(format!(
                "{{\"action\":{},\"enabled\":{enabled},\"supported\":{},\"bounds\":[{},{},{},{}]}}",
                quote(name.as_bytes())?,
                true,
                b[0],
                b[1],
                b[2],
                b[3]
            ));
        }
        if self.popup {
            if let Some(i) = self.hover {
                let b = self.bounds(i as usize);
                let p = &self.images[ACTIONS[i as usize]];
                image.draw(&p.image, [b[0] + p.position[0], b[1] + p.position[1]]);
            }
        }
        let base = &self.images["* BASE"];
        let mut out=format!("{{\"kind\":\"side_menu\",\"size\":[{},{}],\"bounds\":[{},{},{},{}],\"input_sequence\":{},\"action\":{},\"buttons\":[{}]}}\n",self.size[0],self.size[1],base.position[0],base.position[1],base.image.width(),base.image.height(),self.sequence,quote(action.as_bytes())?,buttons.join(",")).into_bytes();
        image.write_png(&mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_quick_load_and_cancelled_drag_do_not_dispatch() {
        let mut images = BTreeMap::new();
        let mut add = |name: String, position| {
            images.insert(
                name,
                Sprite {
                    image: RgbaImage::transparent(10, 8, 320).unwrap(),
                    position,
                },
            );
        };
        add("* BASE".into(), [100, 10]);
        add("- QLOAD".into(), [0, 0]);
        for (i, name) in ACTIONS.iter().enumerate() {
            add(format!("SLOT{i}"), [100, 10 + i as i32 * 10]);
            add(format!("= {name}"), [0, 0]);
            add(name.to_string(), [-10, -5]);
        }
        let mut menu = SideMenu {
            images,
            size: [200, 100],
            quick: false,
            popup: true,
            pressed: None,
            hover: None,
            sequence: 0,
        };
        let action = |packet: Vec<u8>| {
            String::from_utf8(packet.split(|b| *b == 10).next().unwrap().to_vec()).unwrap()
        };
        menu.pointer(0, 101, 21).unwrap();
        assert!(action(menu.pointer(2, 101, 21).unwrap()).contains("\"action\":\"\""));
        menu.pointer(0, 101, 11).unwrap();
        menu.pointer(1, 0, 0).unwrap();
        assert!(action(menu.pointer(2, 101, 11).unwrap()).contains("\"action\":\"\""));
        menu.pointer(0, 101, 11).unwrap();
        assert!(action(menu.pointer(2, 101, 11).unwrap()).contains("\"action\":\"QSAVE\""));
        menu.quick = true;
        menu.pointer(0, 101, 21).unwrap();
        assert!(action(menu.pointer(2, 101, 21).unwrap()).contains("\"action\":\"QLOAD\""));
    }
}
