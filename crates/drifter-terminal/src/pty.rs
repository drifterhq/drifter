use std::ffi::CString;
use std::os::fd::RawFd;

pub struct Pty {
    pub fd: RawFd,
    pub child: libc::pid_t,
}

fn winsize(cols: u16, rows: u16) -> libc::winsize {
    libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 }
}

/// Opens a pseudo-terminal and runs the user's shell on its far end.
pub fn spawn(cols: u16, rows: u16) -> Result<Pty, String> {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    let mut ws = winsize(cols.max(1), rows.max(1));
    let ok = unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &mut ws) };
    if ok != 0 {
        return Err("cannot open a pseudo-terminal".into());
    }

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let shell_c = CString::new(shell.clone()).map_err(|_| "bad shell path")?;
    let argv0 = CString::new(shell.rsplit('/').next().unwrap_or("sh")).unwrap();

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        unsafe {
            libc::close(master);
            libc::close(slave);
        }
        return Err("cannot fork the shell".into());
    }

    if pid == 0 {
        // Child: become the session leader on the slave tty, then exec the shell.
        unsafe {
            libc::close(master);
            libc::setsid();
            libc::ioctl(slave, libc::TIOCSCTTY, 0);
            libc::dup2(slave, 0);
            libc::dup2(slave, 1);
            libc::dup2(slave, 2);
            if slave > 2 {
                libc::close(slave);
            }
            let term = CString::new("TERM").unwrap();
            let xterm = CString::new("xterm-256color").unwrap();
            libc::setenv(term.as_ptr(), xterm.as_ptr(), 1);
            let colorterm = CString::new("COLORTERM").unwrap();
            let truecolor = CString::new("truecolor").unwrap();
            libc::setenv(colorterm.as_ptr(), truecolor.as_ptr(), 1);
            let argv = [argv0.as_ptr(), std::ptr::null()];
            libc::execvp(shell_c.as_ptr(), argv.as_ptr());
            libc::_exit(127);
        }
    }

    // Parent.
    unsafe {
        libc::close(slave);
        let flags = libc::fcntl(master, libc::F_GETFL, 0);
        libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
    Ok(Pty { fd: master, child: pid })
}

pub fn resize(fd: RawFd, cols: u16, rows: u16) {
    let ws = winsize(cols, rows);
    unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &ws) };
}

pub fn write_all(fd: RawFd, mut data: &[u8]) {
    while !data.is_empty() {
        let n = unsafe { libc::write(fd, data.as_ptr() as *const _, data.len()) };
        if n <= 0 {
            break;
        }
        data = &data[n as usize..];
    }
}
