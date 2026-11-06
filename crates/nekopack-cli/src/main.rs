use nekopack_archive::Archive;
use nekopack_image::{decode_png, Mng, RgbaImage};
use nekopack_resource::{resolve, Catalog, Resolution};
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

const INDEX_BUDGET: usize = 16 * 1024 * 1024;
const MNG_BUDGET: u64 = 64 * 1024 * 1024;
const PIXEL_BUDGET: usize = 64 * 1024 * 1024;

fn raster(
    out: &mut impl Write,
    entry: usize,
    image: usize,
    fram: Option<usize>,
    defi: Option<usize>,
    rgba: &RgbaImage,
) -> Result<(), Box<dyn Error>> {
    let optional = |n: Option<usize>| -> Result<u32, std::num::TryFromIntError> {
        n.map(u32::try_from)
            .transpose()
            .map(|v| v.unwrap_or(u32::MAX))
    };
    for n in [
        u32::try_from(entry)?,
        u32::try_from(image)?,
        rgba.width(),
        rgba.height(),
        optional(fram)?,
        optional(defi)?,
        u32::try_from(rgba.pixels().len())?,
    ] {
        out.write_all(&n.to_le_bytes())?;
    }
    out.write_all(rgba.pixels())?;
    Ok(())
}
fn resource(a: &mut Archive<BufReader<File>>, ordinal: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let e = a
        .entries()
        .get(ordinal)
        .ok_or("resource ordinal out of range")?;
    if u64::from(e.length) > MNG_BUDGET {
        return Err("image resource exceeds CLI 64 MiB budget".into());
    }
    let mut data = Vec::new();
    a.copy_entry(ordinal, &mut data)?;
    Ok(data)
}
fn bounded_file(path: &std::ffi::OsStr) -> Result<Vec<u8>, Box<dyn Error>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MNG_BUDGET {
        return Err("image exceeds CLI 64 MiB budget".into());
    }
    let mut data = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(file, MNG_BUDGET + 1), &mut data)?;
    if data.len() as u64 > MNG_BUDGET {
        return Err("image exceeds CLI 64 MiB budget".into());
    }
    Ok(data)
}

fn unhex(s: &std::ffi::OsStr) -> Result<Vec<u8>, Box<dyn Error>> {
    let s = s.to_str().ok_or("hex must be ASCII")?;
    if !s.is_ascii() || s.len() % 2 != 0 {
        return Err("hex needs pairs of ASCII digits".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(Into::into))
        .collect()
}
fn catalog(paths: &[std::ffi::OsString]) -> Result<Catalog, Box<dyn Error>> {
    let mut result = Catalog::default();
    for path in paths {
        let a = open(Path::new(path))?;
        result.add(a.header(), a.entries());
    }
    Ok(result)
}
fn selection(out: &mut impl Write, result: Resolution) -> io::Result<()> {
    match result {
        Resolution::Missing => writeln!(out, "missing"),
        Resolution::Selected(c) => writeln!(out, "selected\t{}\t{}", c.source, c.ordinal),
        Resolution::Ambiguous(c) => writeln!(out, "ambiguous\t{}", c.len()),
    }
}
fn show_mng(out: &mut impl Write, id: usize, data: &[u8]) -> Result<(), Box<dyn Error>> {
    let mng = Mng::parse(data, 100_000)?;
    write!(out, "mng\t{id}")?;
    for v in mng.header {
        write!(out, "\t{v}")?;
    }
    writeln!(out)?;
    for (i, c) in mng.chunks.iter().enumerate() {
        let unknown = matches!(
            c.metadata,
            nekopack_image::Metadata::Unknown { critical: true }
        );
        write!(
            out,
            "chunk\t{id}\t{i}\t{}\t{}\t{}\t{:08X}\t{}\t",
            c.offset,
            std::str::from_utf8(&c.kind)?,
            c.data.len(),
            c.crc,
            u8::from(unknown)
        )?;
        if ![*b"IDAT", *b"JDAT", *b"JDAA"].contains(&c.kind) {
            for b in c.data {
                write!(out, "{b:02X}")?;
            }
        }
        writeln!(out)?;
    }
    Ok(())
}

fn open(path: &Path) -> Result<Archive<BufReader<File>>, Box<dyn Error>> {
    Ok(Archive::open(
        BufReader::new(File::open(path)?),
        INDEX_BUDGET,
    )?)
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 2 {
        return Err(
            "usage: nekopack-cli inspect-archive FILE | read-entry FILE ORDINAL | read-index FILE | hash-name SEED_HEX NAME_BYTES_HEX | lookup-name DIR_BYTES_HEX NAME_BYTES_HEX FILE... | inspect-overlays FILE... | inspect-mng FILE | inspect-mng-archive FILE | decode-png FILE | decode-mng-archive FILE | export-mng-image FILE ENTRY IMAGE | inspect-sprites FILE ENTRY | compose-mng FILE ENTRY:IMAGE... | compose-mng-on FILE RRGGBB ENTRY:IMAGE... | parse-bootstrap FILE ENTRY"
                .into(),
        );
    }
    let command = args[0].to_str().ok_or("invalid command encoding")?;
    match (command, args.len()) {
        ("parse-bootstrap", 3) => {
            let entry = args[2].to_str().ok_or("invalid entry")?.parse::<usize>()?;
            let mut a = open(Path::new(&args[1]))?;
            let data = resource(&mut a, entry)?;
            let nodes = nekopack_script::parse_bootstrap(&data, 100_000)
                .map_err(|e| format!("{} entry {entry}: {e}", Path::new(&args[1]).display()))?;
            let mut out = BufWriter::new(io::stdout().lock());
            for node in nodes {
                write!(out, "{}\t{}\t{}", node.line, node.span.start, node.span.end)?;
                for token in node.tokens {
                    write!(out, "\t")?;
                    for byte in token {
                        write!(out, "{byte:02X}")?;
                    }
                }
                writeln!(out)?;
            }
            out.flush()?;
        }
        ("compose-mng", n) | ("compose-mng-on", n) if n >= 3 => {
            let (background, layers) = if command == "compose-mng-on" {
                let rgb = unhex(&args[2])?;
                if rgb.len() != 3 || n < 4 {
                    return Err("compose-mng-on FILE RRGGBB ENTRY:IMAGE...".into());
                }
                ([rgb[0], rgb[1], rgb[2], 255], &args[3..])
            } else {
                ([0; 4], &args[2..])
            };
            let mut a = open(Path::new(&args[1]))?;
            let mut canvas = None;
            for layer in layers {
                let (entry, image) = layer
                    .to_str()
                    .ok_or("invalid layer")?
                    .split_once(':')
                    .ok_or("layer must be ENTRY:IMAGE")?;
                let data = resource(&mut a, entry.parse()?)?;
                let mng = Mng::parse(&data, 100_000)?;
                let sprites = mng.sprites()?;
                let ordinal: usize = image.parse()?;
                let sprite = sprites.get(ordinal).ok_or("sprite ordinal out of range")?;
                let target = match &mut canvas {
                    Some(target) => target,
                    None => canvas.insert(RgbaImage::filled(
                        mng.header[0],
                        mng.header[1],
                        background,
                        PIXEL_BUDGET,
                    )?),
                };
                if target.width() != mng.header[0] || target.height() != mng.header[1] {
                    return Err("layer canvas dimensions differ".into());
                }
                target.draw(&mng.decode_image(ordinal, PIXEL_BUDGET)?, sprite.position);
            }
            let mut out = BufWriter::new(io::stdout().lock());
            canvas.ok_or("no layers")?.write_png(&mut out)?;
            out.flush()?;
        }
        ("inspect-sprites", 3) => {
            let mut a = open(Path::new(&args[1]))?;
            let data = resource(&mut a, args[2].to_str().ok_or("invalid entry")?.parse()?)?;
            let mng = Mng::parse(&data, 100_000)?;
            for sprite in mng.sprites()? {
                print!(
                    "{}\t{}\t{}\t",
                    sprite.ordinal, sprite.position[0], sprite.position[1]
                );
                for byte in sprite.name {
                    print!("{byte:02X}");
                }
                println!();
            }
        }
        ("decode-png", 2) => {
            let data = bounded_file(&args[1])?;
            let image = decode_png(&data, PIXEL_BUDGET)?;
            let mut out = BufWriter::new(io::stdout().lock());
            out.write_all(b"NRGBA001")?;
            raster(&mut out, 0, 0, None, None, &image)?;
            out.flush()?;
        }
        ("export-mng-image", 4) => {
            let entry = args[2].to_str().ok_or("invalid entry")?.parse::<usize>()?;
            let image = args[3].to_str().ok_or("invalid image")?.parse::<usize>()?;
            let mut a = open(Path::new(&args[1]))?;
            let data = resource(&mut a, entry)?;
            let mng = Mng::parse(&data, 100_000)?;
            let rgba = mng.decode_image(image, PIXEL_BUDGET)?;
            let mut out = BufWriter::new(io::stdout().lock());
            rgba.write_png(&mut out)?;
            out.flush()?;
        }
        ("decode-mng-archive", 2) => {
            let mut a = open(Path::new(&args[1]))?;
            let mut out = BufWriter::new(io::stdout().lock());
            out.write_all(b"NRGBA001")?;
            for entry in 0..a.entries().len() {
                if a.read_prefix(entry, 8)? != b"\x8aMNG\r\n\x1a\n" {
                    continue;
                }
                let data = resource(&mut a, entry)?;
                let mng = Mng::parse(&data, 100_000)?;
                for source in mng.embedded_images()? {
                    let rgba = mng
                        .decode_image(source.ordinal, PIXEL_BUDGET)
                        .map_err(|e| format!("entry {entry} image {}: {e}", source.ordinal))?;
                    raster(
                        &mut out,
                        entry,
                        source.ordinal,
                        source.preceding_fram,
                        source.preceding_defi,
                        &rgba,
                    )?;
                }
            }
            out.flush()?;
        }
        ("hash-name", 3) => {
            let seed = u32::from_str_radix(args[1].to_str().ok_or("invalid seed")?, 16)?;
            println!(
                "{:08X}",
                nekopack_archive::name_hash(seed, &unhex(&args[2])?)
            );
        }
        ("lookup-name", n) if n >= 4 => {
            let candidates = catalog(&args[3..])?.lookup(&unhex(&args[1])?, &unhex(&args[2])?);
            let mut out = BufWriter::new(io::stdout().lock());
            selection(&mut out, resolve(&candidates))?;
            for c in candidates {
                writeln!(
                    out,
                    "candidate\t{}\t{}\t{}",
                    c.source, c.ordinal, c.priority
                )?;
            }
            out.flush()?;
        }
        ("inspect-overlays", n) if n >= 2 => {
            let catalog = catalog(&args[1..])?;
            let mut out = BufWriter::new(io::stdout().lock());
            for ((seed, dir, file), candidates) in catalog.overlaps() {
                write!(out, "overlap\t{seed:08X}\t{dir:08X}\t{file:08X}\t")?;
                selection(&mut out, resolve(&candidates))?;
                for c in candidates {
                    writeln!(
                        out,
                        "candidate\t{}\t{}\t{}",
                        c.source, c.ordinal, c.priority
                    )?;
                }
            }
            out.flush()?;
        }
        ("inspect-mng", 2) => {
            let file = File::open(&args[1])?;
            if file.metadata()?.len() > MNG_BUDGET {
                return Err("MNG exceeds CLI 64 MiB budget".into());
            }
            let mut data = Vec::new();
            // Bound reads even if the file grows after metadata inspection.
            std::io::Read::read_to_end(&mut std::io::Read::take(file, MNG_BUDGET + 1), &mut data)?;
            if data.len() as u64 > MNG_BUDGET {
                return Err("MNG exceeds CLI 64 MiB budget".into());
            }
            let mut out = BufWriter::new(io::stdout().lock());
            show_mng(&mut out, 0, &data)?;
            out.flush()?;
        }
        ("inspect-mng-archive", 2) => {
            let mut a = open(Path::new(&args[1]))?;
            let mut out = BufWriter::new(io::stdout().lock());
            for i in 0..a.entries().len() {
                if a.read_prefix(i, 8)? != b"\x8aMNG\r\n\x1a\n" {
                    continue;
                }
                if u64::from(a.entries()[i].length) > MNG_BUDGET {
                    return Err(format!("entry {i}: MNG exceeds CLI 64 MiB budget").into());
                }
                let mut data = Vec::new();
                a.copy_entry(i, &mut data)?;
                show_mng(&mut out, i, &data).map_err(|e| format!("entry {i}: {e}"))?;
            }
            out.flush()?;
        }
        ("inspect-archive", 2) => {
            let mut archive = open(Path::new(&args[1]))?;
            let h = archive.header();
            let mut out = BufWriter::new(io::stdout().lock());
            writeln!(
                out,
                "archive\t{:08X}\t{}\t{}\t{}\t{}",
                h.name_seed,
                h.priority,
                h.index_length,
                archive.entries().len(),
                archive.trailing_bytes()
            )?;
            writeln!(
                out,
                "ordinal\tdirectory_hash\tname_hash\tblock_offset\tlength\tkey\tdecoded_prefix_hex"
            )?;
            for i in 0..archive.entries().len() {
                let e = archive.entries()[i];
                let prefix = archive.read_prefix(i, 16)?;
                write!(
                    out,
                    "{i}\t{:08X}\t{:08X}\t{}\t{}\t{:08X}\t",
                    e.directory_hash, e.name_hash, e.block_offset, e.length, e.key
                )?;
                for byte in prefix {
                    write!(out, "{byte:02X}")?;
                }
                writeln!(out)?;
            }
            out.flush()?;
        }
        ("read-entry", 3) => {
            let ordinal = args[2]
                .to_str()
                .ok_or("invalid ordinal encoding")?
                .parse::<usize>()?;
            let mut archive = open(Path::new(&args[1]))?;
            let mut out = BufWriter::new(io::stdout().lock());
            archive.copy_entry(ordinal, &mut out)?;
            out.flush()?;
        }
        ("read-index", 2) => {
            let archive = open(Path::new(&args[1]))?;
            io::stdout().lock().write_all(archive.index_bytes())?;
        }
        _ => return Err("unknown command or incorrect argument count".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("nekopack-cli: {error}");
        std::process::exit(1);
    }
}
