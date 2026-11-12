//! Validation-only host: no VM pointer or Rust borrow crosses a JNI call.
use nekopack_script::{runtime::Variables, system::SystemVm};
use std::{error::Error, fmt::Write, path::Path};

// Reuse the exact CLI validation adapter, not a second scenario interpreter.
#[allow(dead_code)]
#[path = "../../../../crates/nekopack-cli/src/title.rs"]
mod title;

fn check(ok: bool, message: &str) -> Result<(), Box<dyn Error>> {
    if ok { Ok(()) } else { Err(message.into()) }
}

fn run(root: &Path) -> Result<String, Box<dyn Error>> {
    let mut report = String::new();
    writeln!(
        report,
        "RUST\t{}\tpointer_bits={}",
        std::env::consts::ARCH,
        std::mem::size_of::<usize>() * 8
    )?;
    let baseline = SystemVm::validation_live_bytes();
    for _ in 0..32 {
        {
            let mut vm = SystemVm::try_new()?;
            let mut vars = Variables::default();
            vm.eval_with_variables("ffi", "def(\"G1\",\"score\"); co <- newthread(function(){var.score=7; suspend(); var.score+=3}); co.call()", &mut vars)?;
            check(vars.get(b"score")? == 7, "callback write")?;
            vars.set(b"score", 20)?;
            vm.eval_with_variables("resume", "co.wakeup()", &mut vars)?;
            check(vars.get(b"score")? == 23, "callback resume")?;
            check(
                vm.eval("unbound", "NekoVariableGet(\"score\")")
                    .unwrap_err()
                    .contains("not bound"),
                "unbind",
            )?;
            vm.validation_panic_next_callback();
            check(
                vm.eval_with_variables(
                    "panic",
                    "try {NekoVariableGet(\"score\")} catch(e) {}",
                    &mut vars,
                )
                .unwrap_err()
                .contains("callback panicked"),
                "panic boundary",
            )?;
            check(
                vm.eval("after-panic", "return 1").is_err(),
                "panic not terminal",
            )?;
        }
        check(SystemVm::validation_live_bytes() == baseline, "FFI VM leak")?;
        for kind in ["memory", "work", "bytecode", "depth"] {
            {
                let mut vm = SystemVm::try_new()?;
                let code = match kind {
                    "memory" => {
                        vm.set_native_limits(vm.memory_used() + 64 * 1024, 4_000_000);
                        "return array(1000000)".to_owned()
                    }
                    "work" => {
                        vm.set_native_limits(64 * 1024 * 1024, 1);
                        "return 1".to_owned()
                    }
                    "bytecode" => {
                        vm.set_instruction_budget(100);
                        "while(true){}".to_owned()
                    }
                    _ => format!("return {}1{}", "(".repeat(200), ")".repeat(200)),
                };
                let error = vm.eval(kind, &code).unwrap_err();
                let expected = match kind {
                    "memory" => "memory budget",
                    "work" => "work budget",
                    "bytecode" => "instruction budget",
                    _ => "recursion limit",
                };
                check(error.contains(expected), &format!("{kind}: {error}"))?;
                check(
                    vm.eval("terminal", "return 1")
                        .unwrap_err()
                        .contains("VM stopped"),
                    "budget not terminal",
                )?;
            }
            check(
                SystemVm::validation_live_bytes() == baseline,
                "budget VM leak",
            )?;
        }
    }
    writeln!(
        report,
        "PASS\tffi-and-budget-destroy\t32 cycles\tlive_bytes={baseline}"
    )?;
    for (case, source) in [
        "local a=[]; for(local i=0;i<80;i++)a.append({value=i}); return a.len().tostring()",
        "local re=regexp(\"a+\"); return re.search(\"aaaa\")!=null ? \"ok\" : \"bad\"",
        "class Node {value=null; constructor(v){value=v}} local a=Node(array(32)); return format(\"%s\",\"ok\")",
    ].iter().enumerate() {
        let mut successful=None;
        for count in 0..2000 {
            let passed={
                let mut vm=SystemVm::try_new()?;
                vm.validation_fail_after(count);
                match vm.eval("allocation",source) {
                    Ok(())=> {check(vm.result()==if case==0 {"80"} else {"ok"},"allocation success result")?;true},
                    Err(error)=> {check(error.contains("native allocation failed"),&error)?;false},
                }
            };
            check(SystemVm::validation_live_bytes()==baseline,"allocation VM leak")?;
            if passed { successful=Some(count);break; }
        }
        let count=successful.ok_or("allocation sweep never completed")?;
        check(count>0,"allocation injection skipped")?;
        writeln!(report,"PASS\tallocation-sweep\t{case}\t{count}")?;
    }
    let results = root.join("results");
    std::fs::create_dir_all(&results)?;
    for (index, child) in ["P01", "012401", "010901", "P01"].iter().enumerate() {
        let fixture = root.join("fixtures").join(index.to_string());
        let mut bytes = Vec::new();
        title::run_to(&fixture, 1, &mut bytes)?;
        let text = String::from_utf8(bytes)?;
        check(text.lines().any(|s| s == "menu-result\t1"), "menu result")?;
        check(
            text.lines().any(|s| {
                s.starts_with(&format!("story-boundary\t{child}\t")) && s.ends_with("\t2")
            }),
            "story boundary",
        )?;
        std::fs::write(results.join(format!("game-{index}.txt")), text)?;
        let mut invalid = Vec::new();
        let error = title::run_to(&fixture, 999, &mut invalid)
            .unwrap_err()
            .to_string();
        check(
            error.contains("requested input does not identify an enabled spot"),
            "invalid choice error",
        )?;
        check(
            !String::from_utf8(invalid)?.contains("menu-result\t"),
            "invalid choice executed",
        )?;
        check(
            SystemVm::validation_live_bytes() == baseline,
            "title VM leak",
        )?;
        writeln!(report, "PASS\ttitle\t{index}\tinvalid-choice-rejected")?;
    }
    writeln!(
        report,
        "PASS\tall\tlive_bytes={}",
        SystemVm::validation_live_bytes()
    )?;
    Ok(report)
}

/// The JNI shim owns input for this synchronous call, and frees output using its exact length.
///
/// # Safety
/// `input` must reference `length` readable bytes; `out_len` must be writable.
#[no_mangle]
pub unsafe extern "C" fn neko_probe_run(
    input: *const u8,
    length: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let result = std::panic::catch_unwind(|| {
        let bytes = unsafe { std::slice::from_raw_parts(input, length) };
        let root = std::str::from_utf8(bytes)?;
        run(Path::new(root))
    });
    let text = match result {
        Ok(Ok(text)) => text,
        Ok(Err(error)) => format!("FAIL\t{error}\n"),
        Err(payload) => {
            std::mem::forget(payload);
            "FAIL\tunexpected Rust panic\n".into()
        }
    };
    let mut bytes = text.into_bytes().into_boxed_slice();
    unsafe {
        *out_len = bytes.len();
    }
    let pointer = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    pointer
}
#[no_mangle]
/// # Safety
/// `bytes` and `length` must be an unfreed pair returned by `neko_probe_run`.
pub unsafe extern "C" fn neko_probe_free(bytes: *mut u8, length: usize) {
    unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            bytes, length,
        )));
    }
}

/// Borrowed descriptors remain owned by Kotlin until this synchronous call returns.
/// # Safety
/// `fds` contains `count` valid open seekable descriptors; out_len is writable.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn neko_probe_dat(
    fds: *const i32,
    count: usize,
    choice: i32,
    out_len: *mut usize,
) -> *mut u8 {
    use std::os::fd::BorrowedFd;
    let result = std::panic::catch_unwind(|| -> Result<String, Box<dyn Error>> {
        let descriptors = unsafe { std::slice::from_raw_parts(fds, count) };
        let mut files = Vec::new();
        for &fd in descriptors {
            files.push(std::fs::File::from(
                unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?,
            ));
        }
        let baseline = SystemVm::validation_live_bytes();
        let mut store = nekopack_runtime::dat::DatGame::mount(files)?;
        let mut output = Vec::new();
        let executed = nekopack_runtime::run_title(&mut store, choice, &mut output);
        check(
            SystemVm::validation_live_bytes() == baseline,
            "DAT title VM leak",
        )?;
        executed?;
        Ok(String::from_utf8(output)?)
    });
    let text = match result {
        Ok(Ok(text)) => text,
        Ok(Err(e)) => format!("FAIL\t{e}\n"),
        Err(payload) => {
            std::mem::forget(payload);
            "FAIL\tunexpected Rust panic\n".into()
        }
    };
    let mut bytes = text.into_bytes().into_boxed_slice();
    unsafe {
        *out_len = bytes.len();
    }
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    ptr
}

#[cfg(target_os = "android")]
thread_local! {static STORY:std::cell::RefCell<Option<nekopack_runtime::story::StorySession<std::fs::File>>>=const {std::cell::RefCell::new(None)};}
#[cfg(target_os = "android")]
thread_local! {static TITLE:std::cell::RefCell<Option<nekopack_runtime::title::TitleSession<std::fs::File>>>=const {std::cell::RefCell::new(None)};}
/// All operations must execute on the same dedicated host worker.
/// # Safety
/// fds is readable for count valid borrowed descriptors; out_len is writable.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn neko_probe_story(
    fds: *const i32,
    count: usize,
    operation: i32,
    sequence: u64,
    elapsed_ms: u64,
    out_len: *mut usize,
) -> *mut u8 {
    use std::os::fd::BorrowedFd;
    let result = std::panic::catch_unwind(|| {
        STORY.with(|cell| -> Result<Vec<u8>, Box<dyn Error>> {
            let mut state = cell.borrow_mut();
            match operation {
                67 => {
                    use nekopack_script::runtime::Scripts;
                    use std::io::Read;
                    let mut files = unsafe { std::slice::from_raw_parts(fds, count) }
                        .iter()
                        .map(|&fd| {
                            unsafe { BorrowedFd::borrow_raw(fd) }
                                .try_clone_to_owned()
                                .map(std::fs::File::from)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let save = if sequence == 1 {
                        if files.len() < 2 {
                            return Err("save and DAT descriptors required".into());
                        }
                        let mut bytes = Vec::new();
                        files
                            .remove(0)
                            .take((nekopack_script::checkpoint::LIMIT + 1) as u64)
                            .read_to_end(&mut bytes)?;
                        Some(bytes)
                    } else {
                        None
                    };
                    let mut store =
                        nekopack_runtime::dat::DatGame::mount_version(files, elapsed_ms != 0)?;
                    let chinese = store.has_chinese_scenario_patch();
                    let patches = store.recognized_chinese_archives();
                    if !store.has_resource("system", "title.mng")? {
                        return Err("missing system/title.mng".into());
                    }
                    store.load(b"start")?;
                    let valid = if let Some(bytes) = save {
                        nekopack_runtime::title::TitleSession::open(store)?
                            .validate_resume(&bytes)
                            .is_ok()
                    } else {
                        false
                    };
                    Ok(format!(
                        "{{\"chinese\":{chinese},\"valid\":{valid},\"patches\":{patches:?}}}"
                    )
                    .into_bytes())
                }
                69 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .override_text_speed(sequence),
                19 => TITLE.with(|title| {
                    let values = unsafe { std::slice::from_raw_parts(fds, count) };
                    let bytes = values
                        .iter()
                        .map(|v| u8::try_from(*v))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut title = title.borrow_mut();
                    let title = title.as_mut().ok_or("no title session")?;
                    title.import_preferences(&bytes)?;
                    title.packet(None)
                }),
                20 => {
                    if let Some(story) = state.as_mut() {
                        story.export_preferences()
                    } else {
                        TITLE.with(|t| {
                            t.borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .export_preferences()
                        })
                    }
                }
                23 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .open_settings(sequence),
                70 => {
                    if let Some(story) = state.as_mut() {
                        story.settings_tick(sequence)
                    } else {
                        TITLE.with(|title| {
                            title.borrow_mut().as_mut().ok_or("no title session")?
                                .settings_tick(sequence)
                        })
                    }
                }
                22 => {
                    if let Some(story) = state.as_mut() {
                        story.settings_command(sequence)
                    } else {
                        TITLE.with(|t| {
                            t.borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .settings_command(sequence)
                        })
                    }
                }
                18 => TITLE.with(|title| {
                    let mut title = title.borrow_mut();
                    if count != 3 {
                        return Err("title pointer requires kind/x/y".into());
                    }
                    let values = unsafe { std::slice::from_raw_parts(fds, count) };
                    if let Some(story) = state.as_mut() {
                        return story.settings_pointer(values[0], values[1], values[2]);
                    }
                    let session = title.as_mut().ok_or("no title session")?;
                    let action = session.pointer(values[0], values[1], values[2])?;
                    if action == Some(1) {
                        let mut session = title.take().unwrap().start()?;
                        session.enable_host_effects();
                        session.enable_interactive_text();
                        *state = Some(session);
                        state.as_mut().unwrap().poll_packet()
                    } else {
                        session.packet_with_settings_format(action, values[0] == 1)
                    }
                }),
                59 | 60 | 61 => TITLE.with(|title| {
                    let mut title = title.borrow_mut();
                    match operation {
                        59 => title
                            .as_mut()
                            .ok_or("no title session")?
                            .open_room(sequence),
                        60 => {
                            if count != 3 {
                                return Err("room pointer requires kind/x/y".into());
                            }
                            let indexes = unsafe { std::slice::from_raw_parts(fds, count) };
                            title
                                .as_mut()
                                .ok_or("no title session")?
                                .room_input(indexes[0], indexes[1], indexes[2])
                        }
                        _ => {
                            let mut session = title.take().ok_or("no title session")?.replay()?;
                            session.enable_host_effects();
                            session.enable_interactive_text();
                            *state = Some(session);
                            state.as_mut().unwrap().poll_packet()
                        }
                    }
                }),
                62 => Ok(state.as_mut().ok_or("no story session")?.export_unlocks()),
                64 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .request_shortcut_reset(sequence),
                65 => state.as_mut().ok_or("no story session")?.shortcut_voice(),
                63 => {
                    let classic = state
                        .as_ref()
                        .ok_or("no story session")?
                        .has_classic_story_menu()?;
                    Ok(format!(r#"{{"classic_story_menu":{classic}}}"#).into_bytes())
                }
                0 | 17 => {
                    if state.is_some() || TITLE.with(|t| t.borrow().is_some()) {
                        return Err("story session already open".into());
                    }
                    let mut files = Vec::new();
                    for &fd in unsafe { std::slice::from_raw_parts(fds, count) } {
                        files.push(std::fs::File::from(
                            unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?,
                        ));
                    }
                    if operation == 17 {
                        use std::io::Read;
                        let mut progress = Vec::new();
                        if elapsed_ms & 1 != 0 {
                            if files.len() < 2 {
                                return Err("progress and DAT descriptors required".into());
                            }
                            files.remove(0).take(65537).read_to_end(&mut progress)?;
                            if progress.len() > 65536 {
                                return Err("unlock progress exceeds size limit".into());
                            }
                        }
                        return TITLE.with(|title| {
                            let mut title = title.borrow_mut();
                            *title =
                                Some(nekopack_runtime::title::TitleSession::open_with_progress(
                                    nekopack_runtime::dat::DatGame::mount_version(
                                        files,
                                        elapsed_ms & 2 != 0,
                                    )?,
                                    sequence != 0,
                                    &progress,
                                )?);
                            title.as_mut().unwrap().packet(None)
                        });
                    }
                    *state = Some(nekopack_runtime::story::StorySession::open(
                        nekopack_runtime::dat::DatGame::mount(files)?,
                        1,
                    )?);
                    state.as_mut().unwrap().enable_host_effects();
                    state.as_mut().unwrap().enable_interactive_text();
                    state.as_mut().unwrap().poll_packet()
                }
                66 if count == 0 => {
                    let ready = if let Some(session) = state.as_mut() {
                        session.quit_autosave_ready()?
                    } else {
                        false
                    };
                    Ok(format!("{{\"ready\":{ready}}}").into_bytes())
                }
                40 | 41 | 46 | 66 => {
                    use std::io::{Read, Write};
                    if operation == 46 && count == 0 {
                        if let Some(session) = state.as_mut() {
                            session.save_ui_slot(sequence as usize, None, String::new())?;
                        } else {
                            TITLE.with(|t| {
                                t.borrow_mut()
                                    .as_mut()
                                    .ok_or("no title session")?
                                    .save_ui_slot(sequence as usize, None, String::new())
                            })?;
                        }
                        return Ok(b"{\"updated\":true}".to_vec());
                    }
                    if count == 0 || (operation != 46 && count != 1) {
                        return Err("save requires a descriptor".into());
                    }
                    let values = unsafe { std::slice::from_raw_parts(fds, count) };
                    let mut file = std::fs::File::from(
                        unsafe { BorrowedFd::borrow_raw(values[0]) }.try_clone_to_owned()?,
                    );
                    if matches!(operation, 40 | 66) {
                        let session = state.as_mut().ok_or("no story session")?;
                        let bytes = if operation == 66 {
                            session.export_quit_autosave()?
                        } else {
                            session.export_save()?
                        };
                        file.write_all(&bytes)?;
                        file.sync_all()?;
                        Ok(format!("{{\"saved\":true,\"bytes\":{}}}", bytes.len()).into_bytes())
                    } else {
                        let mut bytes = Vec::new();
                        file.take((nekopack_script::checkpoint::LIMIT + 1) as u64)
                            .read_to_end(&mut bytes)?;
                        if operation == 46 {
                            let stamp = String::from_utf8(
                                values[1..]
                                    .iter()
                                    .map(|v| u8::try_from(*v))
                                    .collect::<Result<Vec<_>, _>>()?,
                            )?;
                            if let Some(session) = state.as_mut() {
                                session.save_ui_slot(sequence as usize, Some(&bytes), stamp)?;
                            } else {
                                TITLE.with(|t| {
                                    t.borrow_mut()
                                        .as_mut()
                                        .ok_or("no title session")?
                                        .save_ui_slot(sequence as usize, Some(&bytes), stamp)
                                })?;
                            }
                            Ok(b"{\"updated\":true}".to_vec())
                        } else if let Some(session) = state.as_mut() {
                            session.prepare_save(&bytes)
                        } else {
                            TITLE.with(|t| {
                                t.borrow_mut()
                                    .as_mut()
                                    .ok_or("no title session")?
                                    .prepare_save(&bytes)
                            })
                        }
                    }
                }
                42 => {
                    if let Some(session) = state.as_mut() {
                        session.commit_save()
                    } else {
                        let session = TITLE
                            .with(|t| t.borrow_mut().take().ok_or("no title session")?.load())?;
                        *state = Some(session);
                        state.as_mut().unwrap().poll_packet()
                    }
                }
                43 => {
                    if let Some(session) = state.as_mut() {
                        session.cancel_save()
                    } else {
                        TITLE.with(|t| {
                            t.borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .cancel_save()
                        })
                    }
                }
                44 => {
                    if let Some(session) = state.as_mut() {
                        session.open_save_ui(sequence == 1)
                    } else {
                        TITLE.with(|t| {
                            t.borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .open_save_ui()
                        })
                    }
                }
                45 => {
                    if count != 3 {
                        return Err("save pointer needs kind/x/y".into());
                    }
                    let values = unsafe { std::slice::from_raw_parts(fds, count) };
                    if let Some(session) = state.as_mut() {
                        session.save_ui_pointer(values[0], values[1], values[2])
                    } else {
                        TITLE.with(|t| {
                            t.borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .save_ui_pointer(values[0], values[1], values[2])
                        })
                    }
                }
                51 => state.as_mut().ok_or("no story session")?.open_log(sequence),
                52 => {
                    if count != 3 {
                        return Err("log pointer requires kind/x/y".into());
                    }
                    let v = unsafe { std::slice::from_raw_parts(fds, count) };
                    state
                        .as_mut()
                        .ok_or("no story session")?
                        .log_pointer(v[0], v[1], v[2])
                }
                53 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .close_log(sequence),
                54 => {
                    if let Some(s) = state.as_mut() {
                        s.star_sound()
                    } else {
                        TITLE.with(|t| {
                            t.borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .star_sound()
                        })
                    }
                }
                56 => {
                    if count == 0 {
                        return state
                            .as_mut()
                            .ok_or("no story session")?
                            .log_sound(sequence);
                    }
                    if count != 1 {
                        return Err("UI sound requires a volume".into());
                    }
                    let volume = unsafe { *fds };
                    if let Some(story) = state.as_mut() {
                        story.ui_sound(sequence, volume)
                    } else {
                        TITLE.with(|t| {
                            t.borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .ui_sound(sequence, volume)
                        })
                    }
                }
                55 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .log_voice(sequence as usize),
                49 => {
                    use std::io::Read;
                    if count > 1 {
                        return Err("side menu requires optional quick descriptor".into());
                    }
                    let mut bytes = Vec::new();
                    if count == 1 {
                        let fd = unsafe { *fds };
                        let file = std::fs::File::from(
                            unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?,
                        );
                        file.take((nekopack_script::checkpoint::LIMIT + 1) as u64)
                            .read_to_end(&mut bytes)?;
                    }
                    state
                        .as_mut()
                        .ok_or("no story session")?
                        .side_menu(if count == 1 { Some(&bytes) } else { None })
                }
                50 => {
                    if count != 3 {
                        return Err("side pointer needs kind/x/y".into());
                    }
                    let v = unsafe { std::slice::from_raw_parts(fds, count) };
                    state
                        .as_mut()
                        .ok_or("no story session")?
                        .side_pointer(v[0], v[1], v[2])
                }
                48 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .begin_save(sequence),
                47 => {
                    if let Some(session) = state.as_mut() {
                        session.save_ui_packet(0, false)
                    } else {
                        TITLE.with(|t| {
                            t.borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .save_ui_packet(0, false)
                        })
                    }
                }
                1 => state.as_mut().ok_or("no story session")?.poll_packet(),
                9 => state.as_mut().ok_or("no story session")?.audio_config(),
                36 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .open_menu(sequence),
                37 => {
                    if count != 3 {
                        return Err("menu pointer needs kind/x/y".into());
                    }
                    let input = unsafe { std::slice::from_raw_parts(fds, count) };
                    state
                        .as_mut()
                        .ok_or("no story session")?
                        .menu_pointer(sequence, input[0], input[1], input[2])
                }
                38 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .cancel_menu(sequence),
                34 | 35 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .hide_dialogue(sequence, operation == 34),
                33 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .shortcut(elapsed_ms, sequence),
                29 | 30 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .set_skip_mode(sequence, operation == 30),
                31 => Ok(state
                    .as_ref()
                    .ok_or("no story session")?
                    .export_read_history()),
                32 => {
                    if count > 8 * 1024 * 1024 + 5 {
                        return Err("read history too large".into());
                    }
                    let values = unsafe { std::slice::from_raw_parts(fds, count) };
                    let bytes = values
                        .iter()
                        .map(|v| u8::try_from(*v))
                        .collect::<Result<Vec<_>, _>>()?;
                    state
                        .as_mut()
                        .ok_or("no story session")?
                        .import_read_history(&bytes)?;
                    Ok(b"{\"imported\":true}".to_vec())
                }
                25 | 26 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .set_auto_mode(sequence, operation == 26),
                11 | 12 | 27 | 28 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .text_frame_with_voice(
                        sequence,
                        elapsed_ms,
                        operation == 12 || operation == 28,
                        operation == 27 || operation == 28,
                    ),
                13 => state.as_mut().ok_or("no story session")?.text_config(),
                15 | 24 => {
                    use std::io::Read;
                    if count != 1 {
                        return Err("font installation requires one descriptor".into());
                    }
                    let fd = unsafe { *fds };
                    let file = std::fs::File::from(
                        unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?,
                    );
                    let mut bytes = Vec::new();
                    file.take(nekopack_runtime::text_render::MAX_FONT_BYTES as u64 + 1)
                        .read_to_end(&mut bytes)?;
                    if operation == 24 {
                        TITLE.with(|title| -> Result<Vec<u8>, Box<dyn Error>> {
                            title
                                .borrow_mut()
                                .as_mut()
                                .ok_or("no title session")?
                                .install_settings_font(bytes, u32::try_from(sequence)?)?;
                            Ok(b"{\"installed\":true}".to_vec())
                        })
                    } else {
                        state
                            .as_mut()
                            .ok_or("no story session")?
                            .install_dialogue_font(bytes, u32::try_from(sequence)?)
                    }
                }
                16 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .dialogue_image(sequence, elapsed_ms != 0),
                14 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .set_text_config(sequence, elapsed_ms),
                10 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .set_audio_config(sequence, elapsed_ms),
                2 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .acknowledge_text(sequence),
                4 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .acknowledge_effect(sequence),
                8 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .page_frame(sequence, elapsed_ms),
                7 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .acknowledge_movie(sequence, elapsed_ms),
                57 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .acknowledge_selection(sequence, elapsed_ms),
                58 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .replay_dialogue_batch(),
                6 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .flash_frame(sequence, elapsed_ms),
                5 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .effect_frame(sequence, elapsed_ms),
                3 | 39 => {
                    if operation == 39
                        && !state
                            .as_ref()
                            .ok_or("no story session")?
                            .can_close_to_title()
                    {
                        return Err("return title was not requested".into());
                    }
                    TITLE.with(|t| *t.borrow_mut() = None);
                    *state = None;
                    check(
                        SystemVm::validation_live_bytes() == 0,
                        "story close VM leak",
                    )?;
                    if operation == 39 {
                        Ok(format!(
                            "{{\"closed\":true,\"vm_live_bytes\":{}}}\n",
                            SystemVm::validation_live_bytes()
                        )
                        .into_bytes())
                    } else {
                        Ok(b"{\"closed\":true}\n".to_vec())
                    }
                }
                _ => Err("unknown story operation".into()),
            }
        })
    });
    let bytes = match result {
        Ok(Ok(bytes)) => bytes,
        // Save export and preparation are transactional external-input operations.
        // Their ordinary I/O/format errors must not destroy the live scene. Panics
        // and errors after commit still use the fatal cleanup path below.
        Ok(Err(error)) if matches!(operation, 40 | 41 | 46 | 48) => {
            let message = error
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t");
            format!(
                "{{\"recoverable\":true,\"vm_live_bytes\":{},\"error\":\"{message}\"}}\n",
                SystemVm::validation_live_bytes()
            )
            .into_bytes()
        }
        other => {
            TITLE.with(|t| *t.borrow_mut() = None);
            let checkpoint = STORY.with(|cell| {
                let mut state = cell.borrow_mut();
                let checkpoint = state.as_ref().map(|s| s.checkpoint()).transpose();
                *state = None;
                checkpoint
            });
            let message = match other {
                Ok(Err(e)) => e.to_string(),
                Err(payload) => {
                    std::mem::forget(payload);
                    "Rust story callback panic".into()
                }
                _ => unreachable!(),
            };
            let (checkpoint, message) = match checkpoint {
                Ok(state) => (state.unwrap_or("null".into()), message),
                Err(error) => (
                    "null".into(),
                    format!("{message}; checkpoint failed: {error}"),
                ),
            };
            format!(
                "{{\"checkpoint\":{},\"vm_live_bytes\":{},\"error\":\"{}\"}}\n",
                checkpoint,
                SystemVm::validation_live_bytes(),
                message
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t")
            )
            .into_bytes()
        }
    };
    let mut bytes = bytes.into_boxed_slice();
    unsafe {
        *out_len = bytes.len();
    }
    let pointer = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    pointer
}

#[cfg(target_os = "android")]
thread_local! {static MOVIE:std::cell::RefCell<Option<nekopack_video::Decoder<std::fs::File>>>=const {std::cell::RefCell::new(None)};}
/// Movie operations share one dedicated decoder worker, separate from the VM worker.
/// # Safety
/// `fd` is borrowed and valid during open; `out_len` is writable.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn neko_probe_movie(fd: i32, operation: i32, out_len: *mut usize) -> *mut u8 {
    use std::os::fd::BorrowedFd;
    let result = std::panic::catch_unwind(|| {
        MOVIE.with(|cell|->Result<Vec<u8>,Box<dyn Error>> {
        let mut state=cell.borrow_mut();
        match operation {
            0=>{
                check(state.is_none(),"movie already open")?;
                let file=std::fs::File::from(unsafe{BorrowedFd::borrow_raw(fd)}.try_clone_to_owned()?);
                let decoder=nekopack_video::Decoder::open(file)?;
                let i=decoder.info;
                let bytes=format!("{{\"width\":{},\"height\":{},\"fps\":{},\"sample_rate\":{},\"duration\":{}}}\n",i.width,i.height,i.fps,i.sample_rate,i.duration).into_bytes();
                *state=Some(decoder);Ok(bytes)
            }
            1=>{
                let frame=state.as_mut().ok_or("no movie")?.step()?;
                let mut bytes=format!("{{\"pcm_bytes\":{},\"rgba_bytes\":{},\"video_time\":{},\"ended\":{},\"peak_bytes\":{}}}\n",frame.pcm.len()*2,frame.rgba.len(),frame.video_time,frame.ended,frame.peak_bytes).into_bytes();
                for sample in frame.pcm {bytes.extend_from_slice(&sample.to_le_bytes());}
                bytes.extend_from_slice(&frame.rgba);Ok(bytes)
            }
            2=>{*state=None;check(nekopack_video::live_bytes()==0,"movie decoder leak")?;Ok(b"{\"closed\":true,\"decoder_live_bytes\":0}\n".to_vec())}
            _=>Err("unknown movie operation".into())
        }
    })
    });
    let bytes = match result {
        Ok(Ok(bytes)) => bytes,
        failure => {
            MOVIE.with(|cell| *cell.borrow_mut() = None);
            let message = match failure {
                Ok(Err(e)) => e.to_string(),
                Err(p) => {
                    std::mem::forget(p);
                    "movie callback panic".into()
                }
                _ => unreachable!(),
            };
            format!(
                "{{\"error\":\"{}\",\"closed\":true}}\n",
                message
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace(['\n', '\r', '\t'], " ")
            )
            .into_bytes()
        }
    };
    let mut bytes = bytes.into_boxed_slice();
    unsafe { *out_len = bytes.len() };
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    ptr
}
