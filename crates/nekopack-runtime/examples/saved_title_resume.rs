//! Restore a real checkpoint through the title. Print no scenario or image data.
use nekopack_runtime::{dat::DatGame, title::TitleSession};
use std::{error::Error, fs::{self, File}, path::PathBuf};
fn main() -> Result<(), Box<dyn Error>> {
    let args:Vec<_>=std::env::args_os().skip(1).collect();
    if args.len()<2 {return Err("usage: saved_title_resume GAME_DIR SAVE [ARCHIVE_IN_MOUNT_ORDER ...]".into());}
    let mut paths=fs::read_dir(&args[0])?.map(|e|e.map(|e|e.path())).collect::<Result<Vec<PathBuf>,_>>()?;
    paths.retain(|p|p.extension().and_then(|s|s.to_str()).is_some_and(|s|matches!(s,"dat"|"pk2"|"exe")));
    paths.sort();paths.sort_by_key(|p|p.extension().is_some_and(|s|s=="exe"));
    if args.len()>2 {paths=args[2..].iter().map(|name|PathBuf::from(&args[0]).join(name)).collect();}
    let files=paths.iter().map(|p|Ok((p.file_name().ok_or("missing filename")?.to_str().ok_or("invalid filename")?.to_owned(),File::open(p)?))).collect::<Result<Vec<_>,Box<dyn Error>>>()?;
    let mut title=TitleSession::open(DatGame::mount_named(files,true)?)?;
    title.install_settings_font(fs::read(std::env::var("NEKO_TEST_FONT")?)?,0)?;
    title.open_save_ui()?;
    title.prepare_save(&fs::read(&args[1])?)?;
    println!("prepared checkpoint");
    let _story=title.load()?;
    println!("restored title checkpoint");
    Ok(())
}
