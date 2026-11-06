use std::{
    fs::{self, File},
    io::{BufWriter, Write},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    let out = std::path::Path::new(&a[2]);
    fs::create_dir_all(out)?;
    let mut d = nekopack_video::Decoder::open(File::open(&a[1])?)?;
    let i = d.info;
    let (mut frames, mut samples, mut peak) = (0, 0, 0);
    let mut last = Vec::new();
    let mut pcm = BufWriter::new(File::create(out.join("audio.pcm"))?);
    let mut complete_pcm = BufWriter::new(File::create(out.join("complete.pcm"))?);
    loop {
        let f = d.step()?;
        peak = peak.max(f.peak_bytes);
        if !f.rgba.is_empty() {
            if [0, 24, 240].contains(&frames) {
                fs::write(out.join(format!("frame-{frames}.rgba")), &f.rgba)?;
            }
            frames += 1;
            last = f.rgba;
        }
        for s in &f.pcm {
            complete_pcm.write_all(&s.to_le_bytes())?;
            if samples < (i.sample_rate as usize) * 2 * 3 {
                pcm.write_all(&s.to_le_bytes())?;
            }
            samples += 1;
        }
        if f.ended {
            break;
        }
    }
    pcm.flush()?;
    complete_pcm.flush()?;
    fs::write(out.join("last.rgba"), last)?;
    drop(d);
    assert_eq!(nekopack_video::live_bytes(), 0);
    fs::write(out.join("report.json"),format!("{{\"width\":{},\"height\":{},\"fps\":{},\"sample_rate\":{},\"duration\":{},\"frames\":{},\"samples_per_channel\":{},\"peak_bytes\":{}}}\n",i.width,i.height,i.fps,i.sample_rate,i.duration,frames,samples/2,peak))?;
    Ok(())
}
