//! Utilidades comunes para lanzar procesos desde el runtime.
use std::process::Command;

/// `CREATE_NO_WINDOW`: evita que cada hijo abra una consola visible en una app GUI de Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Oculta la consola del proceso hijo en Windows; no hace nada en otras plataformas.
/// El efecto visible solo se comprueba a mano en el build de release (T14).
pub fn hide_console(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Igual que [`hide_console`] para `tokio::process::Command`, con la misma constante.
pub fn hide_console_async(command: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
