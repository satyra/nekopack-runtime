//! Host-side comparison tool. Font and game resources are supplied locally, never embedded.
use nekopack_runtime::text_render::DialogueRenderer;
use std::{env, error::Error, fs};
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    if !(6..=7).contains(&args.len()) {
        return Err("usage: dialogue_overlay FONT MNG SPEAKER_UTF8 TEXT_UTF8 OUTPUT_PREFIX [cp932|gb18030]".into());
    }
    let mut renderer =
        DialogueRenderer::new(fs::read(&args[1])?, 0, &fs::read(&args[2])?, 191, true)?;
    let speaker = fs::read_to_string(&args[3])?;
    let text = fs::read_to_string(&args[4])?;
    let encoding = match args.get(6).map(String::as_str).unwrap_or("cp932") {
        "cp932" => encoding_rs::SHIFT_JIS,
        "gb18030" => encoding_rs::GB18030,
        _ => return Err("unsupported speaker encoding".into()),
    };
    let (name_bytes, _, errors) = encoding.encode(&speaker);
    if errors { return Err("speaker is not representable in the requested encoding".into()); }
    renderer.prepare(1, &speaker, name_bytes.len(), &text)?;
    fs::write(format!("{}-empty.png", args[5]), renderer.frame(1, 0)?)?;
    fs::write(
        format!("{}.png", args[5]),
        renderer.frame(1, text.chars().count())?,
    )?;
    fs::write(format!("{}.json", args[5]), renderer.metadata()?)?;
    Ok(())
}
