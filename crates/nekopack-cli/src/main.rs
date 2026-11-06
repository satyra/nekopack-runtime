use nekopack_archive::Archive;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

const INDEX_BUDGET: usize = 16 * 1024 * 1024;

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
            "usage: nekopack-cli inspect-archive FILE | read-entry FILE ORDINAL | read-index FILE"
                .into(),
        );
    }
    let command = args[0].to_str().ok_or("invalid command encoding")?;
    match (command, args.len()) {
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
