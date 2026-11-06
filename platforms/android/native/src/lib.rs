//! Validation-only host: no VM pointer or Rust borrow crosses a JNI call.
use nekopack_script::{runtime::Variables, system::SystemVm};
use std::{error::Error, fmt::Write, path::Path};

// Reuse the exact CLI validation adapter, not a second scenario interpreter.
#[allow(dead_code)]
#[path = "../../../../crates/nekopack-cli/src/title.rs"]
mod title;

fn check(ok: bool, message: &str) -> Result<(), Box<dyn Error>> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
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
                0 => {
                    if state.is_some() {
                        return Err("story session already open".into());
                    }
                    let mut files = Vec::new();
                    for &fd in unsafe { std::slice::from_raw_parts(fds, count) } {
                        files.push(std::fs::File::from(
                            unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()?,
                        ));
                    }
                    *state = Some(nekopack_runtime::story::StorySession::open(
                        nekopack_runtime::dat::DatGame::mount(files)?,
                        1,
                    )?);
                    state.as_mut().unwrap().enable_host_effects();
                    state.as_mut().unwrap().poll_packet()
                }
                1 => state.as_mut().ok_or("no story session")?.poll_packet(),
                2 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .acknowledge_text(sequence),
                4 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .acknowledge_effect(sequence),
                7 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .acknowledge_movie(sequence, elapsed_ms),
                6 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .flash_frame(sequence, elapsed_ms),
                5 => state
                    .as_mut()
                    .ok_or("no story session")?
                    .effect_frame(sequence, elapsed_ms),
                3 => {
                    *state = None;
                    check(
                        SystemVm::validation_live_bytes() == 0,
                        "story close VM leak",
                    )?;
                    Ok(b"{\"closed\":true}\n".to_vec())
                }
                _ => Err("unknown story operation".into()),
            }
        })
    });
    let bytes = match result {
        Ok(Ok(bytes)) => bytes,
        other => {
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
