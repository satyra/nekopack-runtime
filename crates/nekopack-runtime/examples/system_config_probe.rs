//! Interactive metadata-only original settings probe; never export rendered images.
use nekopack_runtime::{dat::DatGame,title::TitleSession};
use std::{error::Error,fs::File,io::{self,BufRead,Write},path::PathBuf};
fn main()->Result<(),Box<dyn Error>> {
    let directory=std::env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(||".local/system-integration/config-fixture".into());
    let mut paths=std::fs::read_dir(directory)?.map(|e|Ok(e?.path())).collect::<Result<Vec<_>,io::Error>>()?;
    paths.retain(|p|p.extension().is_some_and(|e|e=="dat"||e=="exe"));paths.sort();
    let sources=paths.iter().map(|p|Ok((p.file_name().unwrap().to_str().unwrap().to_owned(),File::open(p)?))).collect::<Result<Vec<_>,io::Error>>()?;
    let title=TitleSession::open(DatGame::mount_named(sources,true)?)?;
    let mut story=title.start()?;story.enable_host_effects();
    story.install_dialogue_font(std::fs::read("C:/Windows/Fonts/msgothic.ttc")?,0)?;
    let mut packet=story.poll_packet()?;let mut elapsed=0;
    let stdin=io::stdin();let mut lines=stdin.lock().lines();
    loop {
        let header=std::str::from_utf8(packet.split(|b|*b==b'\n').next().ok_or("missing header")?)?;
        if !header.contains("\"kind\":\"system_command\""){println!("{{\"returned\":true}}");break;}
        println!("{header}");io::stdout().flush()?;
        let mut line=lines.next().ok_or("probe input ended")??;
        while line=="inspect" {
            println!("{{\"text\":{},\"audio\":{}}}",String::from_utf8(story.text_config()?)?,String::from_utf8(story.audio_config()?)?);io::stdout().flush()?;
            line=lines.next().ok_or("probe input ended")??;
        }
        let args=line.split_whitespace().map(str::parse::<i32>).collect::<Result<Vec<_>,_>>()?;
        let event=*args.first().ok_or("event required")?;
        let pointer=if args.len()==3 {Some([args[1],args[2]])}else{None};
        elapsed+=50;
        if event==4 {std::fs::write(".local/system-integration/config-probe-saved.nkcf",story.export_preferences()?)?;}
        packet=story.system_command_pointer(1,elapsed,event as u32,pointer)?;
    }
    Ok(())
}
