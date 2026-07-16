//! Long-name launcher for the primary `omg` binary.

fn main() {
    let current = std::env::current_exe().expect("failed to locate oh-my-grok executable");
    let binary = current.with_file_name(if cfg!(windows) { "omg.exe" } else { "omg" });
    let mut command = std::process::Command::new(binary);
    command.args(std::env::args_os().skip(1));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        let error = command.exec();
        eprintln!("failed to launch omg: {error}");
        std::process::exit(1);
    }

    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("failed to launch omg: {error}");
            std::process::exit(1);
        }
    }
}
