//! Exercise native partial construction and late reader failure with a private real movie.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};
struct Failing {
    file: File,
    reads: usize,
    limit: usize,
    panic: bool,
}
impl Read for Failing {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.reads += 1;
        if self.reads == self.limit {
            if self.panic {
                panic!("injected movie reader panic")
            };
            return Err(std::io::Error::other("injected movie read failure"));
        }
        self.file.read(bytes)
    }
}
impl Seek for Failing {
    fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(p)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = std::env::args()
        .nth(1)
        .ok_or("expected original movie path")?;
    let mut budget_failures = 0;
    for budget in [
        0,
        1024,
        65536,
        1024 * 1024,
        4 * 1024 * 1024,
        8 * 1024 * 1024,
    ] {
        match nekopack_video::Decoder::with_budget(File::open(&file)?, budget) {
            Err(_) => budget_failures += 1,
            Ok(mut d) => loop {
                match d.step() {
                    Err(_) => {
                        budget_failures += 1;
                        break;
                    }
                    Ok(f) if f.ended => break,
                    Ok(_) => {}
                }
            },
        }
        assert_eq!(nekopack_video::live_bytes(), 0);
    }
    assert!(budget_failures >= 5);
    for panic in [false, true] {
        for limit in [2, 50] {
            let reader = Failing {
                file: File::open(&file)?,
                reads: 0,
                limit,
                panic,
            };
            if let Ok(mut d) = nekopack_video::Decoder::open(reader) {
                loop {
                    match d.step() {
                        Err(_) => break,
                        Ok(f) => assert!(!f.ended, "failure injection never reached"),
                    }
                }
            }
            assert_eq!(nekopack_video::live_bytes(), 0);
        }
    }
    for _ in 0..16 {
        let mut d = nekopack_video::Decoder::open(File::open(&file)?)?;
        for _ in 0..60 {
            d.step()?;
        }
        drop(d);
        assert_eq!(nekopack_video::live_bytes(), 0);
    }
    println!("{{\"allocation_budgets\":6,\"budget_failures\":{budget_failures},\"io_failure_cases\":4,\"early_close_cycles\":16,\"live_bytes\":0}}");
    Ok(())
}
