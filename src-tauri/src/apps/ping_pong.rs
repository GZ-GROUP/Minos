//! Pings a host with the operating system's own `ping` binary and streams its
//! output line by line. The system binary needs no administrator rights on any
//! OS (raw ICMP sockets would), and its output is what users expect to see.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;

use super::error::{AppError, AppResult, ErrorCode};

pub const MAX_COUNT: u32 = 100;
const MAX_HOST_LEN: usize = 253;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PingEvent {
    /// One line of output; `stream` is "out" or "err".
    Line { text: String, stream: &'static str },
    /// The process ended. `code` is None when it was stopped by the user.
    Done { code: Option<i32>, stopped: bool },
}

/// The running `ping` process, so it can be stopped. One run at a time.
#[derive(Default)]
pub struct PingState(Mutex<Option<Child>>);

/// Accepts IPv4, IPv6 and host names. Anything else is rejected before it reaches
/// the command line; a leading "-" would otherwise be read as an option.
pub fn validate_host(host: &str) -> AppResult<&str> {
    let host = host.trim();
    let valid = !host.is_empty()
        && host.len() <= MAX_HOST_LEN
        && !host.starts_with('-')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    if valid {
        Ok(host)
    } else {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            "enter an IP address or host name",
        ))
    }
}

/// `ping` prints in the console's OEM code page on Windows (e.g. 850), so
/// accented text ("Respuesta desde…") must be decoded with it.
#[cfg(windows)]
fn decode(bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{CP_OEMCP, MultiByteToWideChar};
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let Ok(len) = i32::try_from(bytes.len()) else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    // SAFETY: both calls receive the input slice with its exact length; the second
    // writes at most `wide_len` UTF-16 units into a buffer of that size.
    unsafe {
        let wide_len =
            MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0);
        if wide_len <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut wide = vec![0u16; wide_len as usize];
        MultiByteToWideChar(
            CP_OEMCP,
            0,
            bytes.as_ptr(),
            len,
            wide.as_mut_ptr(),
            wide_len,
        );
        String::from_utf16_lossy(&wide)
    }
}

#[cfg(not(windows))]
fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn ping_command(host: &str, count: u32) -> Command {
    let mut command = Command::new("ping");
    let count_flag = if cfg!(windows) { "-n" } else { "-c" };
    // No shell: the host is a single, validated argument.
    command
        .args([count_flag, &count.to_string(), host])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Sends every line from `reader` to the channel until the pipe closes.
fn forward_lines(reader: impl Read, stream: &'static str, channel: &Channel<PingEvent>) {
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    while matches!(reader.read_until(b'\n', &mut buffer), Ok(n) if n > 0) {
        while matches!(buffer.last(), Some(b'\n' | b'\r')) {
            buffer.pop();
        }
        let _ = channel.send(PingEvent::Line {
            text: decode(&buffer),
            stream,
        });
        buffer.clear();
    }
}

fn kill_running(state: &PingState) {
    let previous = state.0.lock().ok().and_then(|mut running| running.take());
    if let Some(mut child) = previous {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[tauri::command]
pub async fn ping_run(
    state: State<'_, PingState>,
    host: String,
    count: u32,
    on_event: Channel<PingEvent>,
) -> AppResult<Option<i32>> {
    let host = validate_host(&host)?.to_string();
    if !(1..=MAX_COUNT).contains(&count) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("the count must be between 1 and {MAX_COUNT}"),
        ));
    }

    kill_running(&state);
    let mut child = ping_command(&host, count).spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Io,
            format!("the ping command isn't available: {error}"),
        )
    })?;
    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    state
        .0
        .lock()
        .map_err(|e| AppError::new(ErrorCode::Io, e.to_string()))?
        .replace(child);

    let errors_channel = on_event.clone();
    let stderr_reader = std::thread::spawn(move || {
        if let Some(stderr) = stderr {
            forward_lines(stderr, "err", &errors_channel);
        }
    });
    let output_channel = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(stdout) = stdout {
            forward_lines(stdout, "out", &output_channel);
        }
    })
    .await
    .map_err(|e| AppError::new(ErrorCode::Io, e.to_string()))?;
    let _ = stderr_reader.join();

    // Pipes are closed, so the process has ended. If it's no longer in the state,
    // `ping_stop` (or a newer run) already killed and reaped it.
    let finished = state
        .0
        .lock()
        .ok()
        .and_then(|mut running| running.take_if(|child| child.id() == pid));
    let code = match finished {
        Some(mut child) => child.wait().ok().and_then(|status| status.code()),
        None => None,
    };
    let stopped = code.is_none();
    let _ = on_event.send(PingEvent::Done { code, stopped });
    Ok(code)
}

#[tauri::command]
pub async fn ping_stop(state: State<'_, PingState>) -> AppResult<()> {
    kill_running(&state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_hosts() {
        for host in [
            "192.168.100.10",
            "example.com",
            "::1",
            "fe80::1",
            "my-pc",
            " 10.0.0.1 ",
        ] {
            assert!(validate_host(host).is_ok(), "{host:?} should be accepted");
        }
        let too_long = "a".repeat(254);
        for host in [
            "",
            " ",
            "-t",
            "-n 9999 x",
            "a b",
            "x;rm",
            "x&&y",
            "$(id)",
            "host/path",
            too_long.as_str(),
        ] {
            assert!(validate_host(host).is_err(), "{host:?} should be rejected");
        }
    }

    /// Real ping to the loopback address: `cargo test -- --ignored`.
    #[test]
    #[ignore = "runs the system ping binary"]
    fn pings_loopback() {
        let mut child = ping_command("127.0.0.1", 1).spawn().unwrap();
        let mut stdout = Vec::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut stdout)
            .unwrap();
        let status = child.wait().unwrap();
        let text = decode(&stdout);
        println!("{text}");
        assert!(status.success());
        assert!(text.contains("127.0.0.1"));
    }
}
