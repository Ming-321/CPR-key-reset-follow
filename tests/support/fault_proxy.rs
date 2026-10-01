//! 仅用于隔离宿主联调：真实清零回执到达后断开插件传输，证明未知结果不重放。
use serde_json::Value;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

fn frame(reader: &mut impl Read) -> std::io::Result<(Value, Vec<u8>)> {
    let mut header = [0u8; 12];
    reader.read_exact(&mut header)?;
    let metadata = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
    let payload = u64::from_be_bytes(header[4..].try_into().unwrap()) as usize;
    if metadata > 65536 || payload > 8 * 1024 * 1024 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0; metadata + payload];
    reader.read_exact(&mut bytes)?;
    let value = serde_json::from_slice(&bytes[..metadata])?;
    let mut raw = header.to_vec();
    raw.extend(bytes);
    Ok((value, raw))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: Value =
        serde_json::from_slice(&std::fs::read("/app/.runtime/data/follow-fault.json")?)?;
    let binary = config["binary"].as_str().ok_or("missing binary")?;
    let marker = config["marker"]
        .as_str()
        .ok_or("missing marker")?
        .to_owned();
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut input = child.stdin.take().ok_or("missing stdin")?;
    let mut output = child.stdout.take().ok_or("missing stdout")?;
    let reset = Arc::new(AtomicU64::new(0));
    let observed = reset.clone();
    std::thread::spawn(move || {
        let mut source = std::io::stdin().lock();
        while let Ok((value, raw)) = frame(&mut source) {
            if value["type"] == "result"
                && value["id"].as_u64() == Some(observed.load(Ordering::SeqCst))
                && OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&marker)
                    .is_ok()
            {
                // 文件只记录注入发生过；不写入账号、Key 或回调载荷。
                let _ = child.kill();
                std::process::exit(99);
            }
            if input.write_all(&raw).and_then(|()| input.flush()).is_err() {
                break;
            }
        }
    });
    let mut target = std::io::stdout().lock();
    while let Ok((value, raw)) = frame(&mut output) {
        if value["type"] == "callback" && value["method"] == "host.keys.reset_budget" {
            reset.store(value["id"].as_u64().unwrap_or(0), Ordering::SeqCst);
        }
        target.write_all(&raw)?;
        target.flush()?;
    }
    Ok(())
}
