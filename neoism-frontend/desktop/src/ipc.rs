#[cfg(unix)]
use neoism_backend::event::{EventProxy, RioEvent, RioEventType};
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io::{self, BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::Duration;

pub const NEW_WINDOW_ARG: &str = "--new-window";
#[cfg(unix)]
const IPC_SOCKET_ENV: &str = "NEOISM_IPC_SOCKET";

#[derive(Debug)]
pub struct ExternalCommandListener {
    #[cfg(unix)]
    path: PathBuf,
}

impl Drop for ExternalCommandListener {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum ExternalCommand {
    NewWindow {
        working_dir: Option<PathBuf>,
        open_paths: Vec<PathBuf>,
    },
}

#[cfg(unix)]
impl ExternalCommand {
    fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end_matches(['\r', '\n']);
        let Some(rest) = line.strip_prefix("new-window") else {
            return None;
        };
        if rest.is_empty() {
            return Some(Self::NewWindow {
                working_dir: None,
                open_paths: Vec::new(),
            });
        }
        let encoded = rest.strip_prefix('\t')?;
        let mut fields = encoded.split('\t');
        let working_dir = match fields.next()? {
            "" => None,
            encoded => percent_decode(encoded).map(PathBuf::from),
        };
        let open_paths = fields
            .map(|encoded| percent_decode(encoded).map(PathBuf::from))
            .collect::<Option<Vec<_>>>()?;
        Some(Self::NewWindow {
            working_dir,
            open_paths,
        })
    }

    fn wire_name(self) -> String {
        match self {
            Self::NewWindow {
                working_dir,
                open_paths,
            } => {
                if working_dir.is_none() && open_paths.is_empty() {
                    return "new-window".to_string();
                }

                let mut encoded = String::from("new-window\t");
                if let Some(path) = working_dir {
                    encoded.push_str(&percent_encode(&path.to_string_lossy()));
                }
                for path in open_paths {
                    encoded.push('\t');
                    encoded.push_str(&percent_encode(&path.to_string_lossy()));
                }
                encoded
            }
        }
    }
}

#[cfg(unix)]
pub fn request_new_window_with_options(
    working_dir: Option<PathBuf>,
    open_paths: Vec<PathBuf>,
) -> io::Result<bool> {
    request_command(ExternalCommand::NewWindow {
        working_dir,
        open_paths,
    })
}

#[cfg(not(unix))]
pub fn request_new_window_with_options(
    _working_dir: Option<std::path::PathBuf>,
    _open_paths: Vec<std::path::PathBuf>,
) -> std::io::Result<bool> {
    Ok(false)
}

#[cfg(unix)]
pub fn listen_for_external_commands(
    event_proxy: EventProxy,
) -> Option<ExternalCommandListener> {
    let path = socket_path();
    let listener = match bind_socket(&path) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::debug!(
                path = %path.display(),
                "external command listener disabled: {err}"
            );
            return None;
        }
    };

    let thread_path = path.clone();
    let spawn_result = std::thread::Builder::new()
        .name("neoism-ipc".to_string())
        .spawn(move || listen_loop(listener, event_proxy));

    if let Err(err) = spawn_result {
        tracing::warn!(
            path = %thread_path.display(),
            "failed to spawn external command listener: {err}"
        );
        let _ = fs::remove_file(&thread_path);
        return None;
    }

    Some(ExternalCommandListener { path })
}

#[cfg(not(unix))]
pub fn listen_for_external_commands(
    _event_proxy: neoism_backend::event::EventProxy,
) -> Option<ExternalCommandListener> {
    None
}

#[cfg(unix)]
fn request_command(command: ExternalCommand) -> io::Result<bool> {
    let path = socket_path();
    let mut stream = match UnixStream::connect(&path) {
        Ok(stream) => stream,
        Err(err) if is_missing_listener(&err) => return Ok(false),
        Err(err) => return Err(err),
    };

    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_millis(500)))?;
    stream.write_all(command.wire_name().as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut response = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut response)?;
    match response.trim_end_matches(['\r', '\n']) {
        "ok" => Ok(true),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unexpected Neoism IPC response: {other}"),
        )),
    }
}

#[cfg(unix)]
fn listen_loop(listener: UnixListener, event_proxy: EventProxy) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                if let Err(err) = handle_stream(&mut stream, &event_proxy) {
                    tracing::debug!("external command failed: {err}");
                }
            }
            Err(err) => {
                tracing::debug!("external command listener accept failed: {err}");
            }
        }
    }
}

#[cfg(unix)]
fn handle_stream(stream: &mut UnixStream, event_proxy: &EventProxy) -> io::Result<()> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut *stream);
        reader.read_line(&mut line)?;
    }

    match ExternalCommand::parse(&line) {
        Some(ExternalCommand::NewWindow {
            working_dir,
            open_paths,
        }) => {
            event_proxy.send_event(
                RioEventType::Rio(RioEvent::CreateWindowWithOptions {
                    working_dir,
                    open_paths,
                }),
                unsafe { neoism_window::window::WindowId::dummy() },
            );
            stream.write_all(b"ok\n")?;
        }
        None => {
            stream.write_all(b"err unknown-command\n")?;
        }
    }
    stream.flush()
}

#[cfg(unix)]
fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'-' | b'_' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(unix)]
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(unix)]
fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }

    if path.exists() {
        match UnixStream::connect(path) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "another Neoism process is already listening",
                ));
            }
            Err(err) if is_missing_listener(&err) => {
                let _ = fs::remove_file(path);
            }
            Err(err) => return Err(err),
        }
    }

    UnixListener::bind(path)
}

#[cfg(unix)]
fn socket_path() -> PathBuf {
    if let Some(socket) = std::env::var_os(IPC_SOCKET_ENV) {
        let path = PathBuf::from(socket);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }

    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("neoism-{}", current_uid()))
        });
    base.join("neoism").join("command.sock")
}

#[cfg(unix)]
fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn is_missing_listener(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn parses_new_window_command() {
        assert_eq!(
            ExternalCommand::parse("new-window\n"),
            Some(ExternalCommand::NewWindow {
                working_dir: None,
                open_paths: Vec::new()
            })
        );
        assert_eq!(
            ExternalCommand::parse("new-window\r\n"),
            Some(ExternalCommand::NewWindow {
                working_dir: None,
                open_paths: Vec::new()
            })
        );
        assert_eq!(ExternalCommand::parse("open\n"), None);
    }

    #[test]
    fn parses_new_window_command_with_working_dir() {
        assert_eq!(
            ExternalCommand::parse("new-window\t/tmp/neoism%20bench\n"),
            Some(ExternalCommand::NewWindow {
                working_dir: Some(PathBuf::from("/tmp/neoism bench")),
                open_paths: Vec::new()
            })
        );
    }

    #[test]
    fn parses_new_window_command_with_open_paths() {
        assert_eq!(
            ExternalCommand::parse(
                "new-window\t/tmp/repo\t/tmp/repo/a%20b.md\t/tmp/c.rs\n"
            ),
            Some(ExternalCommand::NewWindow {
                working_dir: Some(PathBuf::from("/tmp/repo")),
                open_paths: vec![
                    PathBuf::from("/tmp/repo/a b.md"),
                    PathBuf::from("/tmp/c.rs"),
                ],
            })
        );
        assert_eq!(
            ExternalCommand::parse("new-window\t\t/tmp/repo/a.md\n"),
            Some(ExternalCommand::NewWindow {
                working_dir: None,
                open_paths: vec![PathBuf::from("/tmp/repo/a.md")],
            })
        );
    }
}
