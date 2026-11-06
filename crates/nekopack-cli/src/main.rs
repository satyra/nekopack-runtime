use nekopack_archive::Archive;
use nekopack_image::Mng;
use nekopack_resource::{resolve, Catalog, Resolution};
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

const INDEX_BUDGET: usize = 16 * 1024 * 1024;
const MNG_BUDGET: u64 = 64 * 1024 * 1024;

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
            "usage: nekopack-cli inspect-archive FILE | read-entry FILE ORDINAL | read-index FILE | hash-name SEED_HEX NAME_BYTES_HEX | lookup-name DIR_BYTES_HEX NAME_BYTES_HEX FILE... | inspect-overlays FILE... | inspect-mng FILE | inspect-mng-archive FILE"
                .into(),
        );
    }
    let command = args[0].to_str().ok_or("invalid command encoding")?;
    match (command, args.len()) {
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
