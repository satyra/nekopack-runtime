//! Observed Tiny Dungeon static layer profile. Motion/conditions remain unsupported.
use crate::quote;
use nekopack_image::RgbaImage;
use std::collections::BTreeMap;
const RETAINED_BUDGET: usize = 64 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Order {
    Front,
    Back,
    Keep,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekopack_image::decode_png;
    fn layer(slot: u8, color: [u8; 4]) -> Layer {
        Layer {
            filter_pending: false,
            filter: crate::filter::Filter::None,
            slot,
            name: format!("actor{slot}"),
            folder: "image/actor".into(),
            image: RgbaImage::filled(1, 1, color, 4).unwrap(),
            offset: [0, 0],
            anchor: [0, 0],
        }
    }
    fn pixel(s: &mut Scene) -> Vec<u8> {
        decode_png(&s.render().unwrap(), 4)
            .unwrap()
            .pixels()
            .to_vec()
    }
    #[test]
    fn overlapping_layers_respect_order_replacement_and_pruning() {
        let mut s = Scene::new([1, 1]);
        s.put(layer(1, [255, 0, 0, 255]), Order::Front).unwrap();
        s.put(layer(2, [0, 255, 0, 255]), Order::Front).unwrap();
        s.put(layer(3, [0, 0, 255, 255]), Order::Back).unwrap();
        assert_eq!(pixel(&mut s), [0, 255, 0, 255]);
        s.put(layer(1, [255, 255, 0, 255]), Order::Keep).unwrap();
        assert_eq!(pixel(&mut s), [0, 255, 0, 255]);
        s.reorder(&[b"2".to_vec(), b"1".to_vec()]).unwrap();
        assert_eq!(s.actors.len(), 2);
        assert_eq!(pixel(&mut s), [255, 255, 0, 255]);
        assert!(s.reorder(&[b"1".to_vec(), b"1".to_vec()]).is_err());
        assert_eq!(pixel(&mut s), [255, 255, 0, 255]);
        s.reorder(&[]).unwrap();
        assert_eq!(pixel(&mut s), [0, 0, 0, 255]);
    }
    #[test]
    fn filter_is_sampled_on_first_draw_and_cached_until_resource_replacement() {
        use crate::filter::Filter;
        let mut s = Scene::new([1, 1]);
        let mut actor = layer(1, [100, 110, 120, 128]);
        actor.filter_pending = true;
        s.put(actor, Order::Front).unwrap();
        s.filter = Filter::Night;
        s.render().unwrap();
        assert_eq!(s.actors[0].image.pixels(), [83, 88, 137, 128]);
        assert!(!s.actors[0].filter_pending);
        s.filter = Filter::None;
        s.render().unwrap();
        assert_eq!(s.actors[0].image.pixels(), [83, 88, 137, 128]);
        let mut replacement = layer(1, [100, 110, 120, 128]);
        replacement.filter_pending = true;
        s.put(replacement, Order::Keep).unwrap();
        s.render().unwrap();
        assert_eq!(s.actors[0].image.pixels(), [100, 110, 120, 128]);
    }
    #[test]
    fn macros_and_position_are_explicit_and_motion_is_rejected() {
        let macros = BTreeMap::from([("large".into(), vec!["front".into(), "/big".into()])]);
        let p = plan(
            &[b"face".to_vec(), b"large".to_vec(), b"left2".to_vec()],
            &macros,
            640,
        )
        .unwrap();
        assert_eq!(p.name.as_deref(), Some("face"));
        assert_eq!(p.folder.as_deref(), Some("image/actor/big"));
        assert_eq!(p.x, Some(320));
        assert_eq!(plan(&[b"face".to_vec()], &macros, 640).unwrap().x, None);
        for args in [
            vec![b"to".to_vec()],
            vec![b"left9".to_vec()],
            vec![b"clear".to_vec(), b"face".to_vec()],
        ] {
            assert!(plan(&args, &macros, 640).is_err());
        }
    }
}
#[derive(Debug)]
pub(crate) struct Plan {
    pub name: Option<String>,
    pub folder: Option<String>,
    pub x: Option<i32>,
    pub order: Order,
    pub clear: bool,
}
pub(crate) fn plan(
    args: &[Vec<u8>],
    macros: &BTreeMap<String, Vec<String>>,
    center: i32,
) -> Result<Plan, String> {
    let mut words = Vec::new();
    for arg in args {
        let text = std::str::from_utf8(arg).map_err(|e| e.to_string())?;
        if let Some(expansion) = macros.get(text) {
            words.extend(expansion.clone())
        } else {
            words.push(text.to_owned())
        }
        if words.len() > 64 {
            return Err("image argument budget exceeded".into());
        }
    }
    let mut p = Plan {
        name: None,
        folder: None,
        x: None,
        order: Order::Front,
        clear: words.is_empty(),
    };
    for word in &words {
        if macros.contains_key(word) {
            return Err("nested image macros are unsupported".into());
        }
        match word.as_str() {
            "front" => p.order = Order::Front,
            "back" => p.order = Order::Back,
            "keep" => p.order = Order::Keep,
            "center" => p.x = Some(center),
            "/" => p.folder = Some("image/actor".into()),
            "/big" => p.folder = Some("image/actor/big".into()),
            "clear" => {
                if words.len() != 1 {
                    return Err("clear must be the only image argument".into());
                }
                p.clear = true;
            }
            _ => {
                if word.starts_with("left") || word.starts_with("right") {
                    let (suffix, sign) = if let Some(s) = word.strip_prefix("left") {
                        (s, -1)
                    } else {
                        (word.strip_prefix("right").unwrap(), 1)
                    };
                    let rank = match suffix {
                        "" | "1" => 1,
                        "2" => 2,
                        "3" => 3,
                        "4" => 4,
                        _ => return Err("unsupported actor position".into()),
                    };
                    // 160 logical pixels per rank is the observed TD profile, not a device pixel size.
                    p.x = Some(
                        center
                            .checked_add(sign * rank * 160)
                            .ok_or("actor position overflow")?,
                    );
                } else {
                    if ["to", "if", "normal", "flat", "accel", "decel"].contains(&word.as_str())
                        || word.starts_with("slow")
                        || word.starts_with("fast")
                        || word.starts_with([
                            '!', '@', '$', '<', '>', '-', '+', '[', ']', '^', '_', '/',
                        ])
                    {
                        return Err(format!("unsupported actor modifier {word}"));
                    }
                    if p.name.replace(word.clone()).is_some() {
                        return Err("image command has multiple resource names".into());
                    }
                }
            }
        }
    }
    Ok(p)
}
pub(crate) struct Layer {
    pub filter_pending: bool,
    pub filter: crate::filter::Filter,
    pub slot: u8,
    pub name: String,
    pub folder: String,
    pub image: RgbaImage,
    pub offset: [i32; 2],
    pub anchor: [i32; 2],
}
pub(crate) struct Scene {
    pub filter: crate::filter::Filter,
    pub dimensions: [u32; 2],
    pub background: Option<RgbaImage>,
    pub background_name: Option<String>,
    pub background_color: Option<[u8; 4]>,
    pub overlay: Option<[u8; 4]>,
    pub visual: Option<(String, RgbaImage)>,
    pub actors: Vec<Layer>,
}
impl Scene {
    pub fn new(dimensions: [u32; 2]) -> Self {
        Self {
            filter: crate::filter::Filter::None,
            dimensions,
            background: None,
            background_name: None,
            background_color: None,
            overlay: None,
            visual: None,
            actors: Vec::new(),
        }
    }
    pub fn clear(&mut self) {
        self.background = None;
        self.background_name = None;
        self.background_color = None;
        self.overlay = None;
        self.visual = None;
        self.actors.clear();
    }
    fn bytes(&self) -> usize {
        self.visual.as_ref().map_or(0, |(_, i)| i.pixels().len())
            + self.background.as_ref().map_or(0, |i| i.pixels().len())
            + self
                .actors
                .iter()
                .map(|l| l.image.pixels().len())
                .sum::<usize>()
    }
    pub fn set_background(&mut self, name: String, image: RgbaImage) -> Result<(), String> {
        let old = self.background.as_ref().map_or(0, |i| i.pixels().len())
            + self.visual.as_ref().map_or(0, |(_, i)| i.pixels().len());
        if self.bytes() - old + image.pixels().len() > RETAINED_BUDGET {
            return Err("retained scene pixel budget exceeded".into());
        }
        self.background = Some(image);
        self.background_name = Some(name);
        self.background_color = None;
        self.overlay = None;
        self.visual = None;
        Ok(())
    }
    pub fn set_visual(&mut self, name: String, image: RgbaImage) -> Result<(), String> {
        if [image.width(), image.height()] != self.dimensions
            || image
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] != 255)
        {
            return Err("unsupported visual size or transparency".into());
        }
        let old = self.visual.as_ref().map_or(0, |(_, i)| i.pixels().len());
        if self.bytes() - old + image.pixels().len() > RETAINED_BUDGET {
            return Err("retained scene pixel budget exceeded".into());
        }
        self.visual = Some((name, image));
        self.overlay = None;
        Ok(())
    }
    pub fn put(&mut self, layer: Layer, order: Order) -> Result<(), String> {
        let old = self.actors.iter().position(|l| l.slot == layer.slot);
        let old_bytes = old.map_or(0, |i| self.actors[i].image.pixels().len());
        if self.bytes() - old_bytes + layer.image.pixels().len() > RETAINED_BUDGET {
            return Err("retained scene pixel budget exceeded".into());
        }
        if let Some(i) = old {
            self.actors.remove(i);
        }
        let index = match order {
            Order::Front => self.actors.len(),
            Order::Back => 0,
            Order::Keep => old.unwrap_or(self.actors.len()),
        };
        self.actors.insert(index, layer);
        Ok(())
    }
    pub fn reorder(&mut self, args: &[Vec<u8>]) -> Result<(), String> {
        let mut slots = Vec::new();
        for a in args {
            if a.len() != 1 || !(b'1'..=b'9').contains(&a[0]) || slots.contains(&(a[0] - b'0')) {
                return Err("invalid imgx layer list".into());
            }
            slots.push(a[0] - b'0');
        }
        self.actors.retain(|l| slots.contains(&l.slot));
        self.actors
            .sort_by_key(|l| slots.iter().position(|s| *s == l.slot).unwrap());
        Ok(())
    }
    pub fn render(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut canvas = RgbaImage::filled(
            self.dimensions[0],
            self.dimensions[1],
            self.overlay
                .or(self.background_color)
                .unwrap_or([0, 0, 0, 255]),
            RETAINED_BUDGET,
        )?;
        if self.overlay.is_none() {
            if let Some(bg) = &self.background {
                canvas.draw(bg, [0, 0]);
            }
            for l in &mut self.actors {
                if l.filter_pending {
                    self.filter.apply(&mut l.image);
                    l.filter = self.filter;
                    l.filter_pending = false;
                }
                let x = l.anchor[0]
                    .checked_add(l.offset[0])
                    .ok_or("actor x overflow")?;
                let y = l.anchor[1]
                    .checked_add(l.offset[1])
                    .ok_or("actor y overflow")?;
                canvas.draw(&l.image, [x, y]);
            }
        }
        if self.overlay.is_none() {
            if let Some((_, visual)) = &self.visual {
                canvas.draw(visual, [0, 0]);
            }
        }
        let mut png = Vec::new();
        canvas.write_png(&mut png)?;
        Ok(png)
    }
    pub fn metadata(&self) -> Result<String, String> {
        let actors=self.actors.iter().map(|l|Ok(format!("{{\"slot\":{},\"name\":{},\"folder\":{},\"anchor\":[{},{}],\"offset\":[{},{}],\"filter\":{},\"filter_pending\":{}}}",l.slot,quote(l.name.as_bytes())?,quote(l.folder.as_bytes())?,l.anchor[0],l.anchor[1],l.offset[0],l.offset[1],l.filter.metadata(),l.filter_pending))).collect::<Result<Vec<_>,String>>()?;
        let color = |c: Option<[u8; 4]>| {
            c.map_or("null".to_owned(), |c| {
                format!("[{},{},{},{}]", c[0], c[1], c[2], c[3])
            })
        };
        Ok(format!("{{\"size\":[{},{}],\"background\":{},\"background_color\":{},\"overlay\":{},\"visual\":{},\"actors\":[{}],\"filter\":{}}}",self.dimensions[0],self.dimensions[1],self.background_name.as_ref().map(|s|quote(s.as_bytes())).transpose()?.unwrap_or("null".into()),color(self.background_color),color(self.overlay),self.visual.as_ref().map(|(s,_)|quote(s.as_bytes())).transpose()?.unwrap_or("null".into()),actors.join(","),self.filter.metadata()))
    }
}
