//! Minimal static story preview. Audio and timed transitions are recorded, not played.
use crate::{array, bootstrap, dat::DatGame, quote};
use nekopack_image::{decode_png, Mng, RgbaImage};
use nekopack_script::{
    runtime::{Machine, Request, Step},
    system::SystemVm,
};
use std::{
    error::Error,
    io::{Read, Seek},
};
const PIXELS: usize = 64 * 1024 * 1024;
pub struct Page {
    pub sequence: u64,
    pub source: String,
    pub line: usize,
    pub speaker: String,
    pub voice: String,
    pub text: String,
    pub effects: Vec<String>,
    pub png: Vec<u8>,
}
impl Page {
    pub fn metadata(&self) -> Result<String, String> {
        Ok(format!("{{\"sequence\":{},\"source\":{},\"line\":{},\"speaker\":{},\"voice\":{},\"text\":{},\"effects\":{}}}",self.sequence,quote(self.source.as_bytes())?,self.line,quote(self.speaker.as_bytes())?,quote(self.voice.as_bytes())?,quote(self.text.as_bytes())?,array(&self.effects.iter().map(|s|s.as_bytes().to_vec()).collect::<Vec<_>>())?))
    }
    pub fn packet(&self) -> Result<Vec<u8>, String> {
        let mut bytes = self.metadata()?.into_bytes();
        bytes.push(b'\n');
        bytes.extend(&self.png);
        Ok(bytes)
    }
}
pub struct StorySession<R: Read + Seek> {
    store: DatGame<R>,
    machine: Machine,
    system: SystemVm,
    background: Option<RgbaImage>,
    actor: Option<(RgbaImage, [i32; 2])>,
    solid: Option<[u8; 4]>,
    dimensions: [u32; 2],
    page: Option<Page>,
    speaker: String,
    voice: String,
    sequence: u64,
    failed: bool,
}
impl<R: Read + Seek> StorySession<R> {
    pub fn open(mut store: DatGame<R>, choice: i32) -> Result<Self, Box<dyn Error>> {
        let (mut machine, system) = bootstrap(&mut store, choice, &mut std::io::sink())?;
        machine.enable_dialogue();
        let title = store.read("system", "title.mng")?;
        let mng = Mng::parse(&title, 100000)?;
        Ok(Self {
            store,
            machine,
            system,
            background: None,
            actor: None,
            solid: None,
            dimensions: [mng.header[0], mng.header[1]],
            page: None,
            speaker: String::new(),
            voice: String::new(),
            sequence: 0,
            failed: false,
        })
    }
    /// Polling retains the exact pending text. Only a matching sequence acknowledges it.
    pub fn page(&mut self) -> Result<&Page, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        if self.page.is_none() {
            if let Err(e) = self.advance() {
                self.failed = true;
                return Err(e);
            }
        }
        Ok(self.page.as_ref().unwrap())
    }
    pub fn next(&mut self, sequence: u64) -> Result<&Page, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        if self.page.as_ref().map(|p| p.sequence) != Some(sequence) {
            return Err("stale or missing dialogue acknowledgement".into());
        }
        self.machine.complete(None)?;
        self.page = None;
        self.speaker.clear();
        self.voice.clear();
        self.page()
    }
    fn advance(&mut self) -> Result<(), Box<dyn Error>> {
        let mut effects = Vec::new();
        loop {
            match self.machine.advance(&mut self.store)? {
                Step::Finished => return Err("story ended".into()),
                Step::Request(Request::Speaker(fields)) => {
                    self.speaker = String::from_utf8(fields[0].clone())?;
                    self.voice = String::from_utf8(fields[1].clone())?;
                    self.machine.complete(None)?;
                }
                Step::Request(Request::Text(text)) => {
                    let (source, line) = self.machine.location().unwrap();
                    let mut canvas = RgbaImage::filled(
                        self.dimensions[0],
                        self.dimensions[1],
                        self.solid.unwrap_or([0, 0, 0, 255]),
                        PIXELS,
                    )?;
                    if self.solid.is_none() {
                        if let Some(bg) = &self.background {
                            canvas.draw(bg, [0, 0]);
                        }
                        if let Some((actor, pos)) = &self.actor {
                            let x = i32::try_from(self.dimensions[0] / 2)?
                                .checked_add(pos[0])
                                .ok_or("actor position overflow")?;
                            let y = i32::try_from(self.dimensions[1] / 2)?
                                .checked_add(pos[1])
                                .ok_or("actor position overflow")?;
                            canvas.draw(actor, [x, y]);
                        }
                    }
                    let mut png = Vec::new();
                    canvas.write_png(&mut png)?;
                    self.sequence += 1;
                    self.page = Some(Page {
                        sequence: self.sequence,
                        source: String::from_utf8(source.to_vec())?,
                        line,
                        speaker: self.speaker.clone(),
                        voice: self.voice.clone(),
                        text: String::from_utf8(text)?,
                        effects,
                        png,
                    });
                    return Ok(());
                }
                Step::Request(Request::Command { name, args, .. }) => {
                    let command = std::str::from_utf8(&name)?;
                    match command {
                        "title" => {
                            self.system.eval_with_resources(
                                "story-title",
                                &format!("com[\"title\"].call(getroottable(),{})", array(&args)?),
                                &mut self.machine.variables,
                                &mut self.store,
                            )?;
                        }
                        "img0" | "img1" => {
                            if args.len() != 1 {
                                return Err("preview image command requires one name".into());
                            }
                            let folder = if command == "img0" {
                                "image/back"
                            } else {
                                "image/actor"
                            };
                            let name = std::str::from_utf8(&args[0])?;
                            let bytes = self.store.read(folder, &format!("{name}.png"))?;
                            let image = decode_png(&bytes, PIXELS)?;
                            let position = png_position(&bytes)?;
                            if command == "img0" {
                                self.dimensions = [image.width(), image.height()];
                                self.background = Some(image);
                                self.solid = None;
                            } else {
                                self.actor = Some((image, position));
                            }
                        }
                        "img" => {
                            if args.len() != 1 {
                                return Err("preview solid image requires one argument".into());
                            }
                            self.solid = Some(match std::str::from_utf8(&args[0])? {
                                "白" => [255, 255, 255, 255],
                                "黒" => [0, 0, 0, 255],
                                _ => return Err("unsupported preview solid image".into()),
                            });
                        }
                        "bgm" | "se" | "fade" | "wait" => {
                            if args.len() != 1 {
                                return Err("preview effect requires one argument".into());
                            }
                            effects.push(format!("{command} {}", std::str::from_utf8(&args[0])?));
                        }
                        _ => {
                            return Err(
                                format!("unsupported story preview command {command}").into()
                            )
                        }
                    }
                    self.machine.complete(None)?;
                }
                Step::Request(Request::System(_)) => {
                    return Err("unexpected system module in story preview".into())
                }
            }
        }
    }
}
/// PNG pixel offsets are the observed actor placement contract, not MNG timing.
fn png_position(bytes: &[u8]) -> Result<[i32; 2], Box<dyn Error>> {
    let mut at = 8;
    let mut position = [0, 0];
    while at < bytes.len() {
        let header = bytes.get(at..at + 8).ok_or("truncated PNG chunk")?;
        let length = u32::from_be_bytes(header[..4].try_into()?) as usize;
        let end = at
            .checked_add(length)
            .and_then(|n| n.checked_add(12))
            .ok_or("PNG chunk overflow")?;
        let chunk = bytes.get(at..end).ok_or("truncated PNG chunk")?;
        if &header[4..] == b"oFFs" {
            if length != 9 || chunk[16] != 0 {
                return Err("unsupported PNG offsets".into());
            }
            position = [
                i32::from_be_bytes(chunk[8..12].try_into()?),
                i32::from_be_bytes(chunk[12..16].try_into()?),
            ];
        }
        at = end;
    }
    Ok(position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekopack_script::runtime::Source;
    fn session(text: &[u8]) -> StorySession<std::io::Cursor<Vec<u8>>> {
        let mut bytes = b"NEKOPACK".to_vec();
        for n in [0u32, 0, 0, 8, 0, 0] {
            bytes.extend(n.to_le_bytes());
        }
        let store = DatGame::mount([std::io::Cursor::new(bytes)]).unwrap();
        let mut machine = Machine::new(
            Source {
                name: b"test".to_vec(),
                bytes: text.to_vec(),
            },
            100,
            4,
        );
        machine.enable_dialogue();
        StorySession {
            store,
            machine,
            system: SystemVm::new(),
            background: None,
            actor: None,
            solid: None,
            dimensions: [1, 1],
            page: None,
            speaker: String::new(),
            voice: String::new(),
            sequence: 0,
            failed: false,
        }
    }
    #[test]
    fn page_poll_and_stale_input_do_not_advance() {
        let mut s = session(b":Alice:voice:A\nfirst\nsecond\n.unknown\nthird");
        let original = s.page().unwrap().packet().unwrap();
        assert_eq!(s.page().unwrap().packet().unwrap(), original);
        assert!(s.next(0).is_err());
        assert_eq!(s.page().unwrap().packet().unwrap(), original);
        let p = s.next(1).unwrap();
        assert_eq!(p.sequence, 2);
        assert_eq!(p.text, "second");
        assert_eq!(p.speaker, "");
        assert_eq!(p.voice, "");
        assert!(s.next(1).is_err());
        assert_eq!(s.page().unwrap().sequence, 2);
        assert!(s
            .next(2)
            .err()
            .unwrap()
            .to_string()
            .contains("unsupported story preview command"));
        assert!(s.page().err().unwrap().to_string().contains("stopped"));
    }
    #[test]
    fn actor_offsets_use_canvas_center_and_solid_overlay_retains_layers() {
        let mut s = session("one\n.img 白\ntwo\n".as_bytes());
        s.dimensions = [4, 4];
        s.actor = Some((
            RgbaImage::filled(1, 1, [255, 0, 0, 255], PIXELS).unwrap(),
            [-1, -1],
        ));
        let image = decode_png(&s.page().unwrap().png, PIXELS).unwrap();
        assert_eq!(&image.pixels()[20..24], &[255, 0, 0, 255]);
        let image = decode_png(&s.next(1).unwrap().png, PIXELS).unwrap();
        assert!(image.pixels().iter().all(|b| *b == 255));
        assert!(s.actor.is_some());
    }
}
