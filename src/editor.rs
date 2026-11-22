use std::{
    env, fs,
    io::{self, Write},
    path::Path,
    process::{Command, ExitStatus},
};

#[cfg(unix)]
use std::sync::{Arc, atomic::AtomicBool};

use ratatui::{
    DefaultTerminal,
    crossterm::{
        cursor::{Hide, MoveTo, Show},
        execute,
        terminal::{Clear, ClearType, EnterAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};

fn editor_name(configured: Option<&str>) -> String {
    configured
        .map(String::from)
        .into_iter()
        .chain(
            ["VISUAL", "EDITOR"]
                .into_iter()
                .filter_map(|var| env::var(var).ok()),
        )
        .find(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "vi".into())
}

fn editor_command(path: &Path, editor: Option<&str>) -> Command {
    let editor = editor_name(editor);
    if cfg!(windows) {
        let mut command = Command::new("cmd");
        command
            .arg("/C")
            .arg(format!("{editor} \"{}\"", path.display()));
        command
    } else {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(format!("{editor} \"$@\""))
            .arg("sh")
            .arg(path);
        command
    }
}

pub fn edit_text(
    terminal: &mut DefaultTerminal,
    text: &str,
    editor: Option<&str>,
) -> io::Result<Option<String>> {
    let mut file = tempfile::Builder::new().suffix(".md").tempfile()?;
    file.write_all(text.as_bytes())?;
    let path = file.into_temp_path();

    disable_raw_mode()?;
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0), Show)?;

    let status = run_without_interrupts(&mut editor_command(&path, editor));

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, Hide)?;
    terminal.clear()?;

    if !status?.success() {
        return Ok(None);
    }

    let edited = fs::read_to_string(&path)?;
    let edited = edited.strip_suffix('\n').unwrap_or(&edited);
    Ok((edited != text).then(|| edited.to_string()))
}

fn run_without_interrupts(command: &mut Command) -> io::Result<ExitStatus> {
    #[cfg(unix)]
    let guard = signal_hook::flag::register(
        signal_hook::consts::SIGINT,
        Arc::new(AtomicBool::new(false)),
    )?;

    let status = command.status();

    #[cfg(unix)]
    signal_hook::low_level::unregister(guard);

    status
}
