//! Raw-terminal layer for the interactive chat TUI.
//!
//! The real implementation is unix termios; Windows gets compile-clean stubs
//! (the agent binary's job there is serving the desktop agent pane — the
//! standalone TTY chat bails with a clear message until a conhost
//! implementation exists).

#[derive(Debug)]
pub(crate) enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Esc,
    Up,
    Down,
    Left,
    Right,
    CtrlC,
    CtrlD,
    CtrlO,
    CtrlP,
}

#[cfg(unix)]
mod imp {
    use std::collections::VecDeque;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use super::Key;
    use crate::RESET;

    pub(crate) struct RawTerminal {
        original: libc::termios,
    }

    impl RawTerminal {
        pub(crate) fn enter() -> anyhow::Result<Self> {
            let fd = libc::STDIN_FILENO;
            let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
            if unsafe { libc::tcgetattr(fd, &mut original) } != 0 {
                anyhow::bail!("failed to read terminal mode");
            }
            let mut raw = original;
            raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::ISIG | libc::IEXTEN);
            raw.c_iflag &= !(libc::IXON | libc::ICRNL);
            raw.c_cc[libc::VMIN] = 1;
            raw.c_cc[libc::VTIME] = 0;
            if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
                anyhow::bail!("failed to enter raw terminal mode");
            }
            print!("\x1b[?25h");
            std::io::stdout().flush()?;
            Ok(Self { original })
        }
    }

    impl Drop for RawTerminal {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
            }
            let _ = write!(std::io::stdout(), "{RESET}\x1b[r\x1b[?25h");
            let _ = std::io::stdout().flush();
        }
    }

    pub(crate) fn stdin_is_tty() -> bool {
        unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
    }

    pub(crate) fn terminal_size() -> (u16, u16) {
        let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
        if unsafe {
            libc::ioctl(
                libc::STDOUT_FILENO,
                libc::TIOCGWINSZ as libc::c_ulong,
                &mut size,
            )
        } == 0
            && size.ws_col > 0
            && size.ws_row > 0
        {
            (size.ws_col, size.ws_row)
        } else {
            (100, 30)
        }
    }

    fn pending_keys() -> &'static Mutex<VecDeque<Key>> {
        static PENDING: OnceLock<Mutex<VecDeque<Key>>> = OnceLock::new();
        PENDING.get_or_init(|| Mutex::new(VecDeque::new()))
    }

    pub(crate) fn try_read_key() -> anyhow::Result<Option<Key>> {
        if let Some(key) = pending_keys().lock().unwrap().pop_front() {
            return Ok(Some(key));
        }
        let mut poll_fd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll_fd, 1, 0) };
        if ready <= 0 {
            return Ok(None);
        }
        Ok(Some(read_key()?))
    }

    // Read one byte directly from the stdin file descriptor, bypassing Rust's
    // BufReader so that libc::poll on STDIN_FILENO accurately reports whether
    // more bytes are pending.
    fn read_byte_blocking() -> anyhow::Result<u8> {
        let mut buf = [0u8; 1];
        loop {
            let n = unsafe {
                libc::read(libc::STDIN_FILENO, buf.as_mut_ptr() as *mut libc::c_void, 1)
            };
            if n == 1 {
                return Ok(buf[0]);
            }
            if n == -1 {
                let err = std::io::Error::last_os_error();
                if matches!(err.kind(), std::io::ErrorKind::Interrupted) {
                    continue;
                }
                return Err(err.into());
            }
            anyhow::bail!("stdin read returned {n}");
        }
    }

    pub(crate) fn read_key() -> anyhow::Result<Key> {
        let byte = read_byte_blocking()?;
        match byte {
            b'\r' | b'\n' => Ok(Key::Enter),
            b'\t' => Ok(Key::Tab),
            3 => Ok(Key::CtrlC),
            4 => Ok(Key::CtrlD),
            14 => Ok(Key::Down), // Ctrl+N — readline-style fallback for ↓
            15 => Ok(Key::CtrlO),
            16 => Ok(Key::CtrlP), // open /think picker
            8 | 127 => Ok(Key::Backspace),
            27 => read_escape_key(),
            value if value.is_ascii() => Ok(Key::Char(value as char)),
            value => {
                let mut buffer = vec![value];
                while let Some(next) = read_byte_timeout(Duration::from_millis(2))? {
                    buffer.push(next);
                    if std::str::from_utf8(&buffer).is_ok() {
                        break;
                    }
                }
                let text = std::str::from_utf8(&buffer).unwrap_or_default();
                Ok(text.chars().next().map(Key::Char).unwrap_or(Key::Esc))
            }
        }
    }

    fn read_escape_key() -> anyhow::Result<Key> {
        // Some terminals (and tmux/SSH layers) deliver ESC and the rest of a
        // CSI sequence as separate batches. Read everything against a single
        // deadline rather than per-byte timeouts so the parser tolerates that.
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        let mut buffer: Vec<u8> = Vec::with_capacity(8);
        loop {
            let now = std::time::Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(now);
            let Some(byte) = read_byte_timeout(remaining)? else {
                break;
            };
            if byte == 27 && buffer.is_empty() {
                pending_keys().lock().unwrap().push_back(Key::Esc);
                return Ok(Key::Esc);
            }
            buffer.push(byte);
            if buffer.len() >= 2 && (buffer[0] == b'[' || buffer[0] == b'O') {
                let last = *buffer.last().unwrap();
                if (0x40..=0x7E).contains(&last) {
                    break;
                }
            }
            if buffer.len() >= 32 {
                break;
            }
        }
        if buffer.is_empty() {
            return Ok(Key::Esc);
        }
        if buffer[0] != b'[' && buffer[0] != b'O' {
            return Ok(Key::Esc);
        }
        match *buffer.last().unwrap() {
            b'A' => Ok(Key::Up),
            b'B' => Ok(Key::Down),
            b'C' => Ok(Key::Right),
            b'D' => Ok(Key::Left),
            _ => Ok(Key::Esc),
        }
    }

    fn read_byte_timeout(timeout: Duration) -> anyhow::Result<Option<u8>> {
        let mut poll_fd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout_ms = i32::try_from(timeout.as_millis()).unwrap_or(20);
        let ready = unsafe { libc::poll(&mut poll_fd, 1, timeout_ms) };
        if ready <= 0 {
            return Ok(None);
        }
        Ok(Some(read_byte_blocking()?))
    }
}

#[cfg(windows)]
mod imp {
    use super::Key;

    const UNSUPPORTED: &str = "interactive chat isn't supported on Windows yet — use the agent pane in the Neoism app instead";

    pub(crate) struct RawTerminal;

    impl RawTerminal {
        pub(crate) fn enter() -> anyhow::Result<Self> {
            anyhow::bail!(UNSUPPORTED);
        }
    }

    pub(crate) fn stdin_is_tty() -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }

    pub(crate) fn terminal_size() -> (u16, u16) {
        (100, 30)
    }

    pub(crate) fn try_read_key() -> anyhow::Result<Option<Key>> {
        Ok(None)
    }

    pub(crate) fn read_key() -> anyhow::Result<Key> {
        anyhow::bail!(UNSUPPORTED);
    }
}

pub(crate) use imp::*;
