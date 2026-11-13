//! Boot a locally supplied original game to its retained title, without exporting art or text.
use nekopack_runtime::{dat::DatGame, title::TitleSession};
use std::{env, error::Error, fs::File};

fn main() -> Result<(), Box<dyn Error>> {
    let mut paths: Vec<_> = env::args_os().skip(1).collect();
    let translated = paths.iter().any(|arg| arg == "--translated");
    let start = paths.iter().any(|arg| arg == "--start");
    let side = paths.iter().any(|arg| arg == "--side");
    paths.retain(|arg| arg != "--translated" && arg != "--start" && arg != "--side");
    if paths.is_empty() {
        return Err(
            "usage: title_smoke [--translated] [--start] [--side] DAT [DAT_OR_PK2 ...]".into(),
        );
    }
    let files = paths
        .iter()
        .map(|path| {
            Ok((
                std::path::Path::new(path)
                    .file_name()
                    .ok_or("archive filename missing")?
                    .to_str()
                    .ok_or("archive filename is not UTF-8")?
                    .to_owned(),
                File::open(path)?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let store = DatGame::mount_named(files, !translated)?;
    let mut title = TitleSession::open(store).map_err(|e| format!("title open: {e}"))?;
    let packet = title
        .packet(None)
        .map_err(|e| format!("title packet: {e}"))?;
    println!(
        "title-ready archives={} packet_bytes={}",
        paths.len(),
        packet.len()
    );
    let animated = title.settings_tick(3000)?;
    if !animated.is_empty() {
        println!("title-tick-ready packet_bytes={}", animated.len());
    }
    if start || side {
        let mut story = title.start()?;
        if side {
            let packet = story.side_menu(None)?;
            println!("side-ready packet_bytes={}", packet.len());
        }
        story.enable_host_effects();
        let mut packet = story.poll_packet()?;
        for _ in 0..64 {
            let header = std::str::from_utf8(packet.split(|b| *b == b'\n').next().unwrap())?;
            let field = |key: &str| -> Option<&str> {
                let value = header.split(&format!("\"{key}\":")).nth(1)?;
                Some(value.split([',', '}']).next()?.trim_matches('"'))
            };
            if let Some(effect) = field("host_effect") {
                println!("start-effect kind={effect} packet_bytes={}", packet.len());
                let token = field("token").ok_or("effect token missing")?.parse()?;
                packet = story.acknowledge_effect(token)?;
            } else if let Some(sequence) = field("sequence") {
                println!(
                    "start-ready kind=page sequence={sequence} packet_bytes={}",
                    packet.len()
                );
                return Ok(());
            } else {
                return Err("unexpected story packet metadata".into());
            }
        }
        return Err("first-page smoke exceeded 64 host effects".into());
    }
    Ok(())
}
