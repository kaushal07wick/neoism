use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub(crate) fn read_child_output<T>(
    pipe: Option<T>,
) -> tokio::task::JoinHandle<anyhow::Result<Vec<u8>>>
where
    T: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut output = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut output).await?;
        }
        Ok(output)
    })
}

pub(crate) async fn wait_for_cancel(cancel: Option<Arc<AtomicBool>>) {
    let Some(cancel) = cancel else {
        std::future::pending::<()>().await;
        return;
    };
    while !cancel.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub(crate) async fn terminate_child(
    child: &mut tokio::process::Child,
    child_id: Option<u32>,
) {
    terminate_process_group(child_id);
    tokio::time::sleep(Duration::from_millis(100)).await;
    if child.try_wait().ok().flatten().is_none() {
        kill_process_group(child_id);
        let _ = child.start_kill();
    }
    let _ = child.wait().await;
}

#[cfg(unix)]
pub(crate) fn set_new_process_group(command: &mut Command) {
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
}

#[cfg(not(unix))]
pub(crate) fn set_new_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_group(child_id: Option<u32>) {
    if let Some(pid) = child_id {
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGTERM);
        }
    }
}

#[cfg(not(unix))]
fn terminate_process_group(_child_id: Option<u32>) {}

#[cfg(unix)]
fn kill_process_group(child_id: Option<u32>) {
    if let Some(pid) = child_id {
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
fn kill_process_group(_child_id: Option<u32>) {}
