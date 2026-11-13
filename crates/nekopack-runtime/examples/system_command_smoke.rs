//! Exercise an isolated system-handler fixture; print technical metadata only.
use nekopack_runtime::{dat::DatGame, title::TitleSession};
use std::{error::Error, fs::File, path::PathBuf};

fn header(packet: &[u8]) -> Result<&str, Box<dyn Error>> {
    Ok(std::str::from_utf8(
        packet
            .split(|b| *b == b'\n')
            .next()
            .ok_or("missing packet header")?,
    )?)
}
fn field<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    let value = header.split(&format!("\"{name}\":")).nth(1)?;
    Some(value.split([',', '}']).next()?.trim_matches('"'))
}
fn token(packet: &[u8]) -> Result<u64, Box<dyn Error>> {
    Ok(field(header(packet)?, "token")
        .ok_or("missing command token")?
        .parse()?)
}
fn command(packet: &[u8], name: &str) -> Result<bool, Box<dyn Error>> {
    let header = header(packet)?;
    Ok(field(header, "kind") == Some("system_command") && field(header, "command") == Some(name))
}
fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/system-integration/command-fixture"));
    let mut paths = std::fs::read_dir(directory)?
        .map(|entry| Ok(entry?.path()))
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|ext| ext == "dat" || ext == "exe")
    });
    paths.sort();
    let sources = paths
        .iter()
        .map(|path| {
            Ok((
                path.file_name()
                    .ok_or("missing filename")?
                    .to_str()
                    .ok_or("non-UTF8 filename")?
                    .to_owned(),
                File::open(path)?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let title = TitleSession::open(DatGame::mount_named(sources, true)?)?;
    let mut story = title.start()?;
    story.enable_host_effects();
    let mut packet = story.poll_packet()?;
    if !command(&packet, "date")? {
        return Err("fixture did not begin first date".into());
    }
    let first = token(&packet)?;
    let mut elapsed = 0;
    let mut frames = 0;
    while command(&packet, "date")? && token(&packet)? == first {
        elapsed += 50;
        if elapsed > 5000 {
            return Err("first date did not complete on its original timer".into());
        }
        packet = story.system_command_step(first, elapsed, 0)?;
        frames += 1;
    }
    println!("date-auto token={first} elapsed_ms={elapsed} steps={frames}");
    if !command(&packet, "date")? {
        return Err("missing second date".into());
    }
    let second = token(&packet)?;
    if second == first {
        return Err("date requests reused token".into());
    }
    packet = story.system_command_step(second, 100, 1)?;
    if !command(&packet, "probe_wait")? {
        return Err("second date did not finish on confirmation".into());
    }
    println!("date-confirm token={second} elapsed_ms=100");
    let wait = token(&packet)?;
    if field(header(&packet)?, "delay_ms") != Some("-1") {
        return Err("missing first input wait".into());
    }
    if story.system_command_step(wait, 0, 0)? != packet {
        return Err("timer resumed input-only wait".into());
    }
    packet = story.system_command_step(wait, 10, 1)?;
    if !command(&packet, "probe_wait")?
        || token(&packet)? != wait
        || field(header(&packet)?, "delay_ms") != Some("-1")
    {
        return Err("first input failed to reach second wait on same coroutine".into());
    }
    packet = story.system_command_step(wait, 20, 2)?;
    if !command(&packet,"probe_choice")? {return Err("missing registered branch".into());}
    let branch=token(&packet)?;
    story.system_command_step(branch,10,0)?;
    story.system_command_pointer(branch,20,1,Some([360,400]))?;
    packet=story.system_command_pointer(branch,30,0,Some([360,400]))?;
    if field(header(&packet)?, "sequence") != Some("1") {
        return Err("original dialog did not return to second case".into());
    }
    println!("probe-waits token={wait} confirmations=1 cancellations=1 page_sequence=1");
    let sound = story.next_system_command_sound()?;
    let sound_header = header(&sound)?;
    if field(sound_header, "host_effect") != Some("system")
        || field(sound_header, "name") != Some("se_CLICK")
    {
        return Err("missing original closing system sound".into());
    }
    let category = field(sound_header, "category_percent")
        .ok_or("system sound lacks effective category percent")?;
    println!(
        "closing-sound category_percent={category} payload_bytes={}",
        sound.len() - sound_header.len() - 1
    );
    while story.next_system_command_sound()? != b"{\"empty\":true}\n" {}
    if story.next_system_command_sound()? != b"{\"empty\":true}\n" {return Err("system sounds did not drain".into());}
    println!("system-command-smoke passed=true archives={}", paths.len());
    Ok(())
}
