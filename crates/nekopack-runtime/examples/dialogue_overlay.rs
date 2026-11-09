//! Host-side comparison tool. Font and game resources are supplied locally, never embedded.
use nekopack_runtime::text_render::DialogueRenderer;
use std::{env, error::Error, fs};
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 6 {
        return Err("usage: dialogue_overlay FONT MNG SPEAKER_UTF8 TEXT_UTF8 OUTPUT_PREFIX".into());
    }
    let mut renderer =
        DialogueRenderer::new(fs::read(&args[1])?, 0, &fs::read(&args[2])?, 191, true)?;
    let speaker = fs::read_to_string(&args[3])?;
    let text = fs::read_to_string(&args[4])?;
    renderer.prepare(1, &speaker, &text)?;
    fs::write(format!("{}-empty.png", args[5]), renderer.frame(1, 0)?)?;
    fs::write(
        format!("{}.png", args[5]),
        renderer.frame(1, text.chars().count())?,
    )?;
    fs::write(format!("{}.json", args[5]), renderer.metadata()?)?;
    Ok(())
}
