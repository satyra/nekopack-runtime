//! Normal host ACK traversal; writes runtime saves and boundary metadata only.
use nekopack_runtime::{dat::DatGame, story::StorySession, title::TitleSession};
use std::{
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
// Manifest contract: a JSON array of literal filenames (no JSON escapes).
// UTF-8 filenames are accepted; paths and duplicate names are rejected.
fn mount_order(manifest: &Path) -> Result<Vec<String>> {
    let json = fs::read_to_string(manifest)?;
    let mut remaining = json
        .trim()
        .strip_prefix('[')
        .ok_or("mount order must be a JSON filename array")?;
    let mut names = Vec::new();
    loop {
        remaining = remaining.trim_start();
        if let Some(tail) = remaining.strip_prefix(']') {
            if !tail.trim().is_empty() || names.is_empty() {
                return Err("invalid mount order array".into());
            }
            return Ok(names);
        }
        remaining = remaining
            .strip_prefix('"')
            .ok_or("mount order requires JSON strings")?;
        let end = remaining.find('"').ok_or("unterminated mount filename")?;
        let name = &remaining[..end];
        if name.is_empty()
            || name.contains(['\\', '/', ':'])
            || name.chars().any(char::is_control)
            || name == "."
            || name == ".."
        {
            return Err(
                "mount order requires literal filenames without paths or JSON escapes".into(),
            );
        }
        if names.iter().any(|old| old == name) {
            return Err("duplicate mount filename".into());
        }
        names.push(name.to_owned());
        remaining = remaining[end + 1..].trim_start();
        if let Some(tail) = remaining.strip_prefix(',') {
            remaining = tail;
            if remaining.trim_start().starts_with(']') {
                return Err("trailing comma in mount order".into());
            }
        } else if !remaining.starts_with(']') {
            return Err("missing mount order delimiter".into());
        }
    }
}

fn option(args: &mut Vec<std::ffi::OsString>, name: &str) -> Result<Option<std::ffi::OsString>> {
    let Some(index) = args.iter().position(|a| a == name) else {
        return Ok(None);
    };
    if index + 1 == args.len() {
        return Err(format!("missing value for {name}").into());
    }
    args.remove(index);
    Ok(Some(args.remove(index)))
}
fn field<'a>(header: &'a str, key: &str) -> Option<&'a str> {
    Some(
        header
            .split(&format!("\"{key}\":"))
            .nth(1)?
            .split([',', '}'])
            .next()?
            .trim_matches('"'),
    )
}
fn number(header: &str, key: &str) -> Result<u64> {
    Ok(field(header, key)
        .ok_or_else(|| format!("missing {key}"))?
        .parse()?)
}
fn header(packet: &[u8]) -> Result<&str> {
    Ok(std::str::from_utf8(
        packet.split(|b| *b == b'\n').next().ok_or("empty packet")?,
    )?)
}
fn effect(story: &mut StorySession<File>, h: &str) -> Result<Vec<u8>> {
    let token = number(h, "token")?;
    let kind = field(h, "host_effect").ok_or("missing effect kind")?;
    if kind == "flash" {
        for phase in 0..number(h, "duration_ms")? / 30 {
            story.flash_frame(token, phase)?;
        }
    } else if matches!(kind, "shake" | "wipe" | "staffroll") {
        story.effect_frame(token, number(h, "duration_ms")?)?;
    }
    if kind == "movie" {
        Ok(story.acknowledge_movie(token, 0)?)
    } else if kind == "select" {
        Ok(story.acknowledge_selection(token, 1)?)
    } else {
        Ok(story.acknowledge_effect(token)?)
    }
}
fn verify(
    story: &mut StorySession<File>,
    bytes: &[u8],
    expected_source: &str,
    expected_line: u64,
) -> Result<()> {
    let prepared = story.prepare_save(bytes)?;
    let h = header(&prepared)?;
    if field(h, "source") != Some(expected_source) || number(h, "line")? != expected_line {
        return Err("prepared save location mismatch".into());
    }
    let mut packet = story.commit_save()?;
    loop {
        let h = header(&packet)?;
        if field(h, "host_effect").is_some() {
            packet = effect(story, h)?;
        } else {
            if field(h, "source") != Some(expected_source) || number(h, "line")? != expected_line {
                return Err("committed save location mismatch".into());
            }
            let sequence = number(h, "sequence")?;
            story.text_frame(sequence, 0, false)?;
            story.text_frame(sequence, 1_000_000, false)?;
            story.begin_save(sequence)?;
            return Ok(());
        }
    }
}
fn main() -> Result<()> {
    let mut args: Vec<_> = env::args_os().skip(1).collect();
    let manifest = option(&mut args, "--mount-order")?;
    let save_from_line: u64 = option(&mut args, "--save-from-line")?
        .map(|a| a.to_string_lossy().parse())
        .transpose()?
        .unwrap_or(2000);
    let translated = args.iter().any(|a| a == "--translated");
    let resume = args.iter().any(|a| a == "--resume");
    let verify_only = args.iter().any(|a| a == "--verify-only");
    args.retain(|a| a != "--translated" && a != "--resume" && a != "--verify-only");
    if args.len() != 3 {
        return Err(
            "usage: movie_checkpoint GAME_DIR FONT OUT_DIR [--translated] [--resume|--verify-only] [--mount-order MANIFEST_JSON] [--save-from-line LINE]"
                .into(),
        );
    }
    let out = Path::new(&args[2]);
    fs::create_dir_all(out)?;
    let mut log = OpenOptions::new()
        .write(true)
        .create(true)
        .append(resume || verify_only)
        .truncate(!resume && !verify_only)
        .open(out.join("metadata.tsv"))?;
    if !resume && !verify_only {
        writeln!(log, "kind\tsource\tline\tpages\tchoices\tname")?;
    }
    let archives = if let Some(manifest) = manifest {
        let names = mount_order(Path::new(&manifest))?;
        fs::copy(&manifest, out.join("mount-order.json"))?;
        names
            .iter()
            .map(|name| Path::new(&args[0]).join(name))
            .collect::<Vec<_>>()
    } else {
        let mut archives = fs::read_dir(&args[0])?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        archives.retain(|p| {
            p.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                ["dat", "pk2", "exe"]
                    .iter()
                    .any(|e| s.eq_ignore_ascii_case(e))
            })
        });
        archives.sort();
        archives.sort_by_key(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")));
        archives
    };
    let files = archives
        .iter()
        .map(|p| {
            Ok((
                p.file_name()
                    .ok_or("missing filename")?
                    .to_str()
                    .ok_or("non UTF-8 filename")?
                    .to_owned(),
                File::open(p)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let title = TitleSession::open(DatGame::mount_named(files, !translated)?)?;
    let mut story = title.start()?;
    story.install_dialogue_font(fs::read(&args[1])?, 0)?;
    // Preserve normal command execution and scene state while omitting PNG encoding.
    story.enable_route_preview_rendering();
    story.enable_interactive_text();
    story.enable_host_effects();
    let mut packet = story.poll_packet()?;
    let mut latest: Option<(Vec<u8>, String, u64)> = None;
    let mut before_movie: Option<(Vec<u8>, String, u64)> = None;
    let mut pages = 0;
    let mut choices = 0;
    if resume || verify_only {
        while field(header(&packet)?, "host_effect").is_some() {
            packet = effect(&mut story, header(&packet)?)?;
        }
        let sequence = number(header(&packet)?, "sequence")?;
        story.text_frame(sequence, 0, false)?;
        story.begin_save(sequence)?;
        let bytes = fs::read(out.join("before-first-movie.nksv"))?;
        verify(&mut story, &bytes, "CS01_C_06", 2040)?;
        writeln!(
            log,
            "verified-prepare-commit\tCS01_C_06\t2040\t3060\t0\tbefore-first-movie.nksv"
        )?;
        println!("verified before-first-movie.nksv prepare/commit CS01_C_06:2040 translated={translated}");
        if verify_only {
            return Ok(());
        }
        before_movie = Some((bytes, "CS01_C_06".into(), 2040));
        story.cancel_save()?;
        packet = story.poll_packet()?;
        let sequence = number(header(&packet)?, "sequence")?;
        story.text_frame(sequence, 1_000_000, true)?;
        packet = story.acknowledge_text(sequence)?;
        pages = 3060;
        writeln!(
            log,
            "resumed\tCS01_C_06\t2040\t3060\t0\tbefore-first-movie.nksv"
        )?;
    }
    let target;
    loop {
        let h = header(&packet)?;
        if let Some(kind) = field(h, "host_effect") {
            if kind == "select" {
                choices += 1;
            }
            if kind == "movie" {
                let checkpoint = story.checkpoint()?;
                let source = field(&checkpoint, "source").ok_or("movie source missing")?;
                let line = number(&checkpoint, "line")?;
                let name = field(h, "name").ok_or("movie name missing")?;
                writeln!(log, "movie\t{source}\t{line}\t{pages}\t{choices}\t{name}")?;
                log.flush()?;
                if before_movie.is_none() {
                    let saved = latest.take().ok_or("no dialogue before movie")?;
                    fs::write(out.join("before-first-movie.nksv"), &saved.0)?;
                    writeln!(
                        log,
                        "before-movie-save\t{}\t{}\t{pages}\t{choices}\tbefore-first-movie.nksv",
                        saved.1, saved.2
                    )?;
                    before_movie = Some(saved);
                }
                println!("movie source={source} line={line} name={name} pages={pages} choices={choices}; desktop ACK reason=0 only; not played");
            }
            packet = effect(&mut story, h)?;
            continue;
        }
        let sequence = number(h, "sequence")?;
        let source = field(h, "source").ok_or("page source missing")?.to_owned();
        let line = number(h, "line")?;
        pages += 1;
        writeln!(log, "page\t{source}\t{line}\t{pages}\t{choices}\t")?;
        if pages % 50 == 0 {
            println!("page source={source} line={line} pages={pages} choices={choices}");
            log.flush()?;
        }
        story.text_frame(sequence, 0, false)?;
        story.text_frame(sequence, 1_000_000, false)?;
        if source == "CS01_C_16" && line >= 955 {
            story.begin_save(sequence)?;
            let bytes = story.export_save()?;
            fs::write(out.join("restored-progress.nksv"), &bytes)?;
            writeln!(
                log,
                "progress-save\t{source}\t{line}\t{pages}\t{choices}\trestored-progress.nksv"
            )?;
            target = (bytes, source, line);
            break;
        }
        if before_movie.is_none() && source == "CS01_C_06" && line >= save_from_line {
            story.begin_save(sequence)?;
            latest = Some((story.export_save()?, source, line));
            story.cancel_save()?;
        }
        story.text_frame(sequence, 1_000_000, true)?;
        packet = story.acknowledge_text(sequence)?;
        if pages > 20000 {
            return Err("target not reached within 20000 normal pages".into());
        }
    }
    verify(&mut story, &target.0, &target.1, target.2)?;
    writeln!(
        log,
        "verified-prepare-commit\t{}\t{}\t{pages}\t{choices}\trestored-progress.nksv",
        target.1, target.2
    )?;
    let saved = before_movie.ok_or("first movie missing")?;
    verify(&mut story, &saved.0, &saved.1, saved.2)?;
    writeln!(
        log,
        "verified-prepare-commit\t{}\t{}\t{pages}\t{choices}\tbefore-first-movie.nksv",
        saved.1, saved.2
    )?;
    println!("complete translated={translated} pages={pages} choices={choices} progress={}:{} before_movie={}:{}", target.1,target.2,saved.1,saved.2);
    Ok(())
}
