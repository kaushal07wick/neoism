use std::collections::HashMap;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
#[cfg(unix)]
use std::{io, ptr};

use axum::extract::ws::{Message, WebSocket};
use neoism_agent_core::PtyInfo;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
#[cfg(not(unix))]
use tokio::process::ChildStdin;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};
use tracing::warn;

use super::pty_buffer::{PtyOutputBuffer, PtyOutputEvent};
use super::{PtyError, PtySize};

pub(crate) async fn stop_pty_process(pty_id: &str) {
    process_registry().stop(pty_id).await;
}

pub(crate) async fn resize_pty_process(pty_id: &str, size: PtySize) {
    process_registry().resize(pty_id, size).await;
}

pub(crate) async fn stop_all_pty_processes() {
    process_registry().stop_all().await;
}

pub(crate) async fn serve_websocket(
    info: PtyInfo,
    cursor: Option<i64>,
    mut socket: WebSocket,
    on_exit: impl Fn(String, Option<i32>) + Send + Sync + 'static,
) {
    let process = match process_registry()
        .get_or_spawn(info.clone(), Arc::new(on_exit))
        .await
    {
        Ok(process) => process,
        Err(error) => {
            let _ = socket
                .send(Message::Text(format!(
                    "failed to start PTY process: {error:?}"
                )))
                .await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };

    let mut output_cursor = if cursor.unwrap_or_default() >= 0 {
        cursor.unwrap_or_default() as u64
    } else {
        process.buffer.lock().await.cursor()
    };

    if cursor.unwrap_or_default() >= 0 {
        let replay = process.buffer.lock().await.replay_from(output_cursor);
        for chunk in replay {
            if send_output(&mut socket, &chunk.data, chunk.cursor)
                .await
                .is_err()
            {
                return;
            }
            output_cursor = chunk.cursor;
        }
    }

    if send_cursor(&mut socket, output_cursor).await.is_err() {
        return;
    }

    let mut output = process.output.subscribe();
    if process.exited.load(Ordering::SeqCst) {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(message) = message else {
                    break;
                };
                match message {
                    Ok(Message::Text(data)) => {
                        if write_stdin(&process.stdin, data.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Binary(data)) => {
                        if data.first() == Some(&0) {
                            continue;
                        }
                        if write_stdin(&process.stdin, &data).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Ping(data)) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Pong(_)) => {}
                    Ok(Message::Close(_)) => break,
                    Err(_) => break,
                }
            }
            event = output.recv() => {
                match event {
                    Ok(PtyOutputEvent::Data(chunk)) => {
                        if chunk.cursor <= output_cursor {
                            continue;
                        }
                        if send_output(&mut socket, &chunk.data, chunk.cursor).await.is_err() {
                            break;
                        }
                        output_cursor = chunk.cursor;
                    }
                    Ok(PtyOutputEvent::Exited) => {
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let replay = process.buffer.lock().await.replay_from(output_cursor);
                        for chunk in replay {
                            if send_output(&mut socket, &chunk.data, chunk.cursor).await.is_err() {
                                return;
                            }
                            output_cursor = chunk.cursor;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

struct PtyProcess {
    stdin: Arc<Mutex<PtyInput>>,
    child: Arc<Mutex<Child>>,
    #[cfg(unix)]
    pgid: Option<libc::pid_t>,
    output: broadcast::Sender<PtyOutputEvent>,
    buffer: Arc<Mutex<PtyOutputBuffer>>,
    exited: AtomicBool,
}

enum PtyInput {
    #[cfg(not(unix))]
    Pipe(ChildStdin),
    #[cfg(unix)]
    Pty(tokio::fs::File),
}

#[derive(Default)]
struct PtyProcessRegistry {
    processes: Mutex<HashMap<String, Arc<PtyProcess>>>,
}

impl PtyProcessRegistry {
    async fn get_or_spawn(
        &'static self,
        info: PtyInfo,
        on_exit: Arc<dyn Fn(String, Option<i32>) + Send + Sync>,
    ) -> Result<Arc<PtyProcess>, PtyError> {
        let mut processes = self.processes.lock().await;
        if let Some(process) = processes.get(&info.id) {
            return Ok(process.clone());
        }

        let process = spawn_process(info.clone(), on_exit)?;
        processes.insert(info.id, process.clone());
        Ok(process)
    }

    async fn remove_if_same(&self, pty_id: &str, process: &Arc<PtyProcess>) {
        let mut processes = self.processes.lock().await;
        if processes
            .get(pty_id)
            .is_some_and(|current| Arc::ptr_eq(current, process))
        {
            processes.remove(pty_id);
        }
    }

    async fn stop(&self, pty_id: &str) {
        let process = self.processes.lock().await.remove(pty_id);
        if let Some(process) = process {
            stop_process(process).await;
        }
    }

    async fn resize(&self, pty_id: &str, size: PtySize) {
        let process = self.processes.lock().await.get(pty_id).cloned();
        if let Some(process) = process {
            let _ = resize_process(process, size).await;
        }
    }

    async fn stop_all(&self) {
        let processes = self.processes.lock().await.drain().collect::<Vec<_>>();
        for (_, process) in processes {
            stop_process(process).await;
        }
    }
}

fn process_registry() -> &'static PtyProcessRegistry {
    static REGISTRY: OnceLock<PtyProcessRegistry> = OnceLock::new();
    REGISTRY.get_or_init(PtyProcessRegistry::default)
}

async fn stop_process(process: Arc<PtyProcess>) {
    process.exited.store(true, Ordering::SeqCst);
    #[cfg(unix)]
    {
        signal_process_group(&process, libc::SIGHUP);
        signal_process_group(&process, libc::SIGTERM);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut child = process.child.lock().await;
    let _ = child.start_kill();
    let _ = process.output.send(PtyOutputEvent::Exited);
}

#[cfg(unix)]
fn signal_process_group(process: &PtyProcess, signal: libc::c_int) {
    if let Some(pgid) = process.pgid {
        let rc = unsafe { libc::kill(-pgid, signal) };
        if rc < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                warn!(pgid, signal, error = %error, "failed to signal PTY process group");
            }
        }
    }
}

#[cfg(unix)]
fn spawn_process(
    info: PtyInfo,
    on_exit: Arc<dyn Fn(String, Option<i32>) + Send + Sync>,
) -> Result<Arc<PtyProcess>, PtyError> {
    spawn_pty_process(info, on_exit)
}

#[cfg(not(unix))]
fn spawn_process(
    info: PtyInfo,
    on_exit: Arc<dyn Fn(String, Option<i32>) + Send + Sync>,
) -> Result<Arc<PtyProcess>, PtyError> {
    spawn_pipe_process(info, on_exit)
}

#[cfg(not(unix))]
fn spawn_pipe_process(
    info: PtyInfo,
    on_exit: Arc<dyn Fn(String, Option<i32>) + Send + Sync>,
) -> Result<Arc<PtyProcess>, PtyError> {
    let command = info.command.first().ok_or_else(|| {
        PtyError::SpawnFailed(
            "PTY command must contain at least one argument".to_string(),
        )
    })?;
    let mut process = Command::new(command);
    process
        .args(info.command.iter().skip(1))
        .current_dir(&info.cwd)
        .env("TERM", "xterm-256color")
        .env("NEOISM_TERMINAL", "1")
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = process
        .spawn()
        .map_err(|error| PtyError::SpawnFailed(error.to_string()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| PtyError::Io("failed to open process stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PtyError::Io("failed to open process stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| PtyError::Io("failed to open process stderr".to_string()))?;

    let (output, _) = broadcast::channel(1024);
    let buffer = Arc::new(Mutex::new(PtyOutputBuffer::default()));
    let child = Arc::new(Mutex::new(child));
    let pty_id = info.id.clone();
    let process = Arc::new(PtyProcess {
        stdin: Arc::new(Mutex::new(PtyInput::Pipe(stdin))),
        child: child.clone(),
        output: output.clone(),
        buffer: buffer.clone(),
        exited: AtomicBool::new(false),
    });

    tokio::spawn(read_process_output(
        stdout,
        output.clone(),
        buffer.clone(),
        pty_id.clone(),
        "stdout",
    ));
    tokio::spawn(read_process_output(
        stderr,
        output.clone(),
        buffer,
        pty_id.clone(),
        "stderr",
    ));

    let monitor_ref = process.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let status = {
                let mut child = child.lock().await;
                child.try_wait()
            };
            match status {
                Ok(Some(status)) => {
                    let code = status.code();
                    let already_exited = monitor_ref.exited.swap(true, Ordering::SeqCst);
                    let _ = output.send(PtyOutputEvent::Exited);
                    process_registry()
                        .remove_if_same(&pty_id, &monitor_ref)
                        .await;
                    if !already_exited {
                        on_exit(pty_id, code);
                    }
                    return;
                }
                Ok(None) => continue,
                Err(error) => {
                    warn!(pty_id = %pty_id, error = %error, "failed to poll PTY process");
                    break;
                }
            }
        }
        let already_exited = monitor_ref.exited.swap(true, Ordering::SeqCst);
        let _ = output.send(PtyOutputEvent::Exited);
        process_registry()
            .remove_if_same(&pty_id, &monitor_ref)
            .await;
        if !already_exited {
            on_exit(pty_id, None);
        }
    });

    Ok(process)
}

#[cfg(unix)]
fn spawn_pty_process(
    info: PtyInfo,
    on_exit: Arc<dyn Fn(String, Option<i32>) + Send + Sync>,
) -> Result<Arc<PtyProcess>, PtyError> {
    let command = info.command.first().ok_or_else(|| {
        PtyError::SpawnFailed(
            "PTY command must contain at least one argument".to_string(),
        )
    })?;
    let (master_fd, slave_fd) = open_pty(PtySize { cols: 80, rows: 24 })?;
    let writer_fd = unsafe { libc::dup(master_fd) };
    if writer_fd < 0 {
        unsafe {
            libc::close(master_fd);
            libc::close(slave_fd);
        }
        return Err(PtyError::Io(io::Error::last_os_error().to_string()));
    }
    if let Err(error) = set_cloexec(master_fd).and_then(|_| set_cloexec(writer_fd)) {
        unsafe {
            libc::close(master_fd);
            libc::close(writer_fd);
            libc::close(slave_fd);
        }
        return Err(error);
    }

    let mut process = Command::new(command);
    process
        .args(info.command.iter().skip(1))
        .current_dir(&info.cwd)
        .env("TERM", "xterm-256color")
        .env("NEOISM_TERMINAL", "1")
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    unsafe {
        process.as_std_mut().pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(slave_fd, libc::TIOCSCTTY as libc::c_ulong, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::tcsetpgrp(slave_fd, libc::getpid()) < 0 {
                return Err(io::Error::last_os_error());
            }
            for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
                if libc::dup2(slave_fd, fd) < 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            if slave_fd > libc::STDERR_FILENO {
                libc::close(slave_fd);
            }
            Ok(())
        });
    }

    let child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            unsafe {
                libc::close(master_fd);
                libc::close(writer_fd);
                libc::close(slave_fd);
            }
            return Err(PtyError::SpawnFailed(error.to_string()));
        }
    };
    unsafe {
        libc::close(slave_fd);
    }

    let reader = unsafe { std::fs::File::from_raw_fd(master_fd) };
    let writer = unsafe { std::fs::File::from_raw_fd(writer_fd) };
    let (output, _) = broadcast::channel(1024);
    let buffer = Arc::new(Mutex::new(PtyOutputBuffer::default()));
    let pgid = child.id().map(|pid| pid as libc::pid_t);
    let child = Arc::new(Mutex::new(child));
    let pty_id = info.id.clone();
    let process = Arc::new(PtyProcess {
        stdin: Arc::new(Mutex::new(PtyInput::Pty(tokio::fs::File::from_std(writer)))),
        child: child.clone(),
        pgid,
        output: output.clone(),
        buffer: buffer.clone(),
        exited: AtomicBool::new(false),
    });

    tokio::spawn(read_process_output(
        tokio::fs::File::from_std(reader),
        output.clone(),
        buffer,
        pty_id.clone(),
        "pty",
    ));

    monitor_process(child, process.clone(), output, pty_id, on_exit);
    Ok(process)
}

#[cfg(unix)]
fn open_pty(size: PtySize) -> Result<(libc::c_int, libc::c_int), PtyError> {
    let mut master = 0;
    let mut slave = 0;
    let mut winsize = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut winsize,
        )
    };
    if rc < 0 {
        return Err(PtyError::Io(io::Error::last_os_error().to_string()));
    }
    Ok((master, slave))
}

#[cfg(unix)]
fn set_cloexec(fd: libc::c_int) -> Result<(), PtyError> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(PtyError::Io(io::Error::last_os_error().to_string()));
    }
    let rc = unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
    if rc < 0 {
        return Err(PtyError::Io(io::Error::last_os_error().to_string()));
    }
    Ok(())
}

fn monitor_process(
    child: Arc<Mutex<Child>>,
    process: Arc<PtyProcess>,
    output: broadcast::Sender<PtyOutputEvent>,
    pty_id: String,
    on_exit: Arc<dyn Fn(String, Option<i32>) + Send + Sync>,
) {
    tokio::spawn(async move {
        let mut code = None;
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let status = {
                let mut child = child.lock().await;
                child.try_wait()
            };
            match status {
                Ok(Some(status)) => {
                    code = status.code();
                    break;
                }
                Ok(None) => continue,
                Err(error) => {
                    warn!(pty_id = %pty_id, error = %error, "failed to poll PTY process");
                    break;
                }
            }
        }
        let already_exited = process.exited.swap(true, Ordering::SeqCst);
        let _ = output.send(PtyOutputEvent::Exited);
        process_registry().remove_if_same(&pty_id, &process).await;
        if !already_exited {
            on_exit(pty_id, code);
        }
    });
}

async fn read_process_output<R>(
    mut reader: R,
    output: broadcast::Sender<PtyOutputEvent>,
    buffer: Arc<Mutex<PtyOutputBuffer>>,
    pty_id: String,
    stream: &'static str,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut bytes = [0; 8192];
    loop {
        match reader.read(&mut bytes).await {
            Ok(0) => break,
            Ok(n) => {
                let data = String::from_utf8_lossy(&bytes[..n]).to_string();
                let chunk = buffer.lock().await.push(data);
                let _ = output.send(PtyOutputEvent::Data(chunk));
            }
            Err(error) => {
                warn!(pty_id = %pty_id, stream, error = %error, "failed to read PTY process output");
                break;
            }
        }
    }
}

async fn resize_process(process: Arc<PtyProcess>, size: PtySize) -> Result<(), PtyError> {
    #[cfg(unix)]
    {
        let input = process.stdin.lock().await;
        let PtyInput::Pty(file) = &*input;
        let mut winsize = libc::winsize {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let rc = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                libc::TIOCSWINSZ as libc::c_ulong,
                &mut winsize,
            )
        };
        if rc < 0 {
            return Err(PtyError::Io(io::Error::last_os_error().to_string()));
        }
        signal_process_group(&process, libc::SIGWINCH);
    }
    #[cfg(not(unix))]
    {
        let _ = (process, size);
    }
    Ok(())
}

async fn write_stdin(stdin: &Arc<Mutex<PtyInput>>, data: &[u8]) -> Result<(), PtyError> {
    let mut input = stdin.lock().await;
    match &mut *input {
        #[cfg(not(unix))]
        PtyInput::Pipe(stdin) => {
            stdin
                .write_all(data)
                .await
                .map_err(|error| PtyError::Io(error.to_string()))?;
            stdin
                .flush()
                .await
                .map_err(|error| PtyError::Io(error.to_string()))
        }
        #[cfg(unix)]
        PtyInput::Pty(file) => {
            file.write_all(data)
                .await
                .map_err(|error| PtyError::Io(error.to_string()))?;
            file.flush()
                .await
                .map_err(|error| PtyError::Io(error.to_string()))
        }
    }
}

async fn send_output(
    socket: &mut WebSocket,
    data: &str,
    cursor: u64,
) -> Result<(), axum::Error> {
    socket.send(Message::Text(data.to_string())).await?;
    send_cursor(socket, cursor).await
}

async fn send_cursor(socket: &mut WebSocket, cursor: u64) -> Result<(), axum::Error> {
    let mut payload = Vec::with_capacity(32);
    payload.push(0);
    payload.extend_from_slice(format!(r#"{{"cursor":{cursor}}}"#).as_bytes());
    socket.send(Message::Binary(payload)).await
}
