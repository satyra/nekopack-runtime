//! Original nested room probe: print metadata and audio byte counts, never images.
//! `state 0|1` injects platform playback state; this CLI does not play audio.
use nekopack_runtime::{dat::DatGame,title::TitleSession};
use std::{error::Error,fs::File,io::{self,BufRead,Write},path::PathBuf};

fn header(packet:&[u8])->Result<&str,Box<dyn Error>> {
    Ok(std::str::from_utf8(packet.split(|b|*b==b'\n').next().ok_or("missing packet header")?)?)
}
fn main()->Result<(),Box<dyn Error>> {
    let directory=std::env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(||".local/system-integration/nested-room-fixture".into());
    let mut paths=std::fs::read_dir(directory)?.map(|e|Ok(e?.path())).collect::<Result<Vec<_>,io::Error>>()?;
    paths.retain(|p|p.extension().is_some_and(|e|e=="dat"||e=="exe"));paths.sort();
    let sources=paths.iter().map(|p|Ok((p.file_name().unwrap().to_str().unwrap().to_owned(),File::open(p)?))).collect::<Result<Vec<_>,io::Error>>()?;
    let title=TitleSession::open(DatGame::mount_named(sources,true)?)?;
    let mut story=title.start()?;story.enable_host_effects();
    story.install_dialogue_font(std::fs::read("C:/Windows/Fonts/msgothic.ttc")?,0)?;
    let mut packet=story.poll_packet()?;let mut elapsed=0;
    let stdin=io::stdin();let mut lines=stdin.lock().lines();
    loop {
        loop {
            let audio=story.next_system_room_audio()?;
            let meta=header(&audio)?;
            if meta=="{\"empty\":true}" {break;}
            let payload=audio.len()-meta.len()-1;
            println!("{{\"audio\":{meta},\"payload_bytes\":{payload}}}");
        }
        let meta=header(&packet)?;
        if !meta.contains("\"kind\":\"system_command\""){println!("{{\"returned\":true}}");break;}
        println!("{meta}");io::stdout().flush()?;
        // Never enter production replay here: only report its action boundary.
        if meta.contains("\"room_replay\":true") {break;}
        let token=meta.split("\"token\":").nth(1).ok_or("missing token")?.split(',').next().unwrap().parse::<u64>()?;
        if meta.contains("\"load_catalog\":true") {
            story.system_load_catalog_clear()?;
            story.system_load_catalog_install()?;
            packet=story.system_command_step(token,elapsed,6)?;
            continue;
        }
        let mut line=lines.next().ok_or("probe input ended")??;
        while let Some(state)=line.strip_prefix("state ") {
            let playing=match state {"0"=>false,"1"=>true,_=>return Err("state must be 0 or 1".into())};
            story.system_room_music_state(playing)?;
            line=lines.next().ok_or("probe input ended")??;
        }
        if line=="end" {break;}
        let args=line.split_whitespace().map(str::parse::<i32>).collect::<Result<Vec<_>,_>>()?;
        let event=*args.first().ok_or("event required")?;
        let pointer=match args.len(){1=>None,3=>Some([args[1],args[2]]),_=>return Err("event or event x y required".into())};
        elapsed+=50;
        packet=story.system_command_pointer(token,elapsed,event as u32,pointer)?;
    }
    Ok(())
}
