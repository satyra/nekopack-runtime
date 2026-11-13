//! In-memory NEXT/BACK smoke; prints boundary metadata, never story text or art.
use nekopack_runtime::{dat::DatGame, story::StorySession, title::TitleSession};
use std::{
    env,
    error::Error,
    fs::{self, File},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug)]
enum Boundary {
    Page(u64),
    Selection(u64),
}

// Only inspect the JSON header. Effect payloads may contain choice strings and
// page payloads contain rendered pixels; neither is decoded or printed.
fn field<'a>(header: &'a str, key: &str) -> Option<&'a str> {
    let value = header.split(&format!("\"{key}\":")).nth(1)?;
    Some(value.split([',', '}']).next()?.trim_matches('"'))
}
fn number(header: &str, key: &str) -> Result<u64> {
    Ok(field(header, key)
        .ok_or_else(|| format!("missing metadata {key}"))?
        .parse()?)
}

fn boundary(story: &mut StorySession<File>, mut packet: Vec<u8>) -> Result<Boundary> {
    for index in 0..4096 {
        let header = std::str::from_utf8(
            packet
                .split(|b| *b == b'\n')
                .next()
                .ok_or("missing packet header")?,
        )?;
        if let Some(kind) = field(header, "host_effect") {
            let token = number(header, "token")?;
            if index % 64 == 0 && kind != "select" {
                println!("effect-boundary index={index} token={token} kind={kind}");
            }
            if kind == "select" {
                println!(
                    "selection token={token} source={} line={} steps={}",
                    field(header, "source").ok_or("selection source missing")?,
                    number(header, "line")?,
                    number(header, "steps")?
                );
                return Ok(Boundary::Selection(token));
            }
            // A movie is completed without presenting or exporting its bytes.
            if kind == "flash" {
                for phase in 0..number(header, "duration_ms")? / 30 {
                    story.flash_frame(token, phase)?;
                }
            } else if matches!(kind, "shake" | "wipe" | "staffroll") {
                story.effect_frame(token, number(header, "duration_ms")?)?;
            }
            packet = if kind == "movie" {
                story.acknowledge_movie(token, 0)?
            } else {
                story.acknowledge_effect(token)?
            };
        } else if field(header, "sequence").is_some() {
            let sequence = number(header, "sequence")?;
            println!(
                "page sequence={sequence} source={} line={} steps={}",
                field(header, "source").ok_or("page source missing")?,
                number(header, "line")?,
                number(header, "steps")?
            );
            return Ok(Boundary::Page(sequence));
        } else {
            return Err("unexpected boundary metadata".into());
        }
    }
    Err("navigation smoke exceeded 4096 host effects".into())
}

fn first_page(story: &mut StorySession<File>, packet: Vec<u8>) -> Result<u64> {
    let mut pending = boundary(story, packet)?;
    for _ in 0..128 {
        match pending {
            Boundary::Page(sequence) => {
                story.text_frame(sequence, 0, false)?;
                return Ok(sequence);
            }
            Boundary::Selection(token) => {
                println!("choose token={token} option_id=1");
                let packet = story.acknowledge_selection(token, 1)?;
                pending = boundary(story, packet)?;
            }
        }
    }
    Err("navigation smoke exceeded 128 consecutive selections".into())
}

fn main() -> Result<()> {
    let mut args: Vec<_> = env::args_os().skip(1).collect();
    let translated = args.iter().any(|arg| arg == "--translated");
    args.retain(|arg| arg != "--translated");
    if args.len() != 2 {
        return Err("usage: navigation_smoke GAME_DIR FONT [--translated]".into());
    }
    let mut archives = fs::read_dir(&args[0])?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    archives.retain(|path| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                ext.eq_ignore_ascii_case("dat")
                    || ext.eq_ignore_ascii_case("pk2")
                    || ext.eq_ignore_ascii_case("exe")
            })
    });
    archives.sort();
    archives.sort_by_key(|path| {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    });
    if archives.is_empty() {
        return Err("no DAT/PK2 archives in game directory".into());
    }
    let files = archives
        .iter()
        .map(|path| {
            Ok((
                path.file_name()
                    .ok_or("archive filename missing")?
                    .to_str()
                    .ok_or("archive filename is not UTF-8")?
                    .to_owned(),
                File::open(path)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let store = DatGame::mount_named(files, !translated)?;
    let title = TitleSession::open(store)?;
    println!(
        "title-ready archives={} translated={translated}",
        archives.len()
    );
    let mut story = title.start()?;
    story.install_dialogue_font(fs::read(&args[1])?, 0)?;
    story.enable_interactive_text();
    story.enable_host_effects();
    let packet = story.poll_packet()?;
    let initial = first_page(&mut story, packet)?;

    let packet = story.begin_next(initial)?;
    let Boundary::Selection(token) = boundary(&mut story, packet)? else {
        return Err("NEXT returned a dialogue before reaching selection".into());
    };
    println!("next-ready token={token}");
    let packet = story.acknowledge_selection(token, 1)?;
    let after_choice = first_page(&mut story, packet)?;

    let packet = story.request_shortcut_back(after_choice)?;
    let Boundary::Selection(restored) = boundary(&mut story, packet)? else {
        return Err("BACK returned a dialogue before restoring selection".into());
    };
    println!("back-ready token={restored}");
    let packet = story.acknowledge_selection(restored, 1)?;
    let mut sequence = first_page(&mut story, packet)?;
    for _ in 0..3 {
        story.text_frame(sequence, 1_000_000, false)?;
        story.text_frame(sequence, 1_000_000, true)?;
        let packet = story.acknowledge_text(sequence)?;
        sequence = first_page(&mut story, packet)?;
    }
    println!("navigation-smoke passed final_sequence={sequence}");
    Ok(())
}
