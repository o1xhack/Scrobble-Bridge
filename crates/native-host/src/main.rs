use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::Command,
    thread,
    time::Duration,
};

use interprocess::local_socket::{Stream, traits::Stream as _};
use scrobble_ipc::{IpcRequest, IpcResponse, MAX_MESSAGE_BYTES, local_socket_name};
use thiserror::Error;

#[derive(Debug, Error)]
enum HostError {
    #[error("native message is too large")]
    MessageTooLarge,
    #[error("native message ended unexpectedly")]
    UnexpectedEnd,
    #[error("native message is invalid")]
    InvalidMessage,
    #[error("desktop App is not available")]
    DesktopUnavailable,
    #[error(transparent)]
    Io(#[from] io::Error),
}

fn main() -> Result<(), HostError> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();

    loop {
        let Some(payload) = read_chrome_message(&mut input)? else {
            return Ok(());
        };
        let response = process_message(&payload).unwrap_or_else(|error| {
            IpcResponse::failure(match error {
                HostError::DesktopUnavailable => {
                    "Open Scrobble Bridge, then try refreshing credentials again."
                }
                _ => "Scrobble Bridge could not accept this credential snapshot.",
            })
        });
        write_chrome_message(&mut output, &response)?;
    }
}

fn process_message(payload: &[u8]) -> Result<IpcResponse, HostError> {
    let request: IpcRequest =
        serde_json::from_slice(payload).map_err(|_| HostError::InvalidMessage)?;
    request.validate().map_err(|_| HostError::InvalidMessage)?;
    let serialized = serde_json::to_string(&request).map_err(|_| HostError::InvalidMessage)?;
    if let Ok(response) = send_to_desktop(&serialized) {
        Ok(response)
    } else {
        launch_desktop();
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(100));
            if let Ok(response) = send_to_desktop(&serialized) {
                return Ok(response);
            }
        }
        Err(HostError::DesktopUnavailable)
    }
}

fn send_to_desktop(serialized: &str) -> Result<IpcResponse, HostError> {
    let mut connection = BufReader::new(Stream::connect(
        local_socket_name().map_err(|error| io::Error::other(error.to_string()))?,
    )?);
    connection.get_mut().write_all(serialized.as_bytes())?;
    connection.get_mut().write_all(b"\n")?;
    connection.get_mut().flush()?;
    let mut response = String::new();
    connection.read_line(&mut response)?;
    serde_json::from_str(&response).map_err(|_| HostError::InvalidMessage)
}

fn read_chrome_message(reader: &mut impl Read) -> Result<Option<Vec<u8>>, HostError> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let length = usize::try_from(u32::from_le_bytes(length)).unwrap_or(usize::MAX);
    if length > MAX_MESSAGE_BYTES {
        return Err(HostError::MessageTooLarge);
    }
    let mut payload = vec![0; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| match error.kind() {
            io::ErrorKind::UnexpectedEof => HostError::UnexpectedEnd,
            _ => HostError::Io(error),
        })?;
    Ok(Some(payload))
}

fn write_chrome_message(writer: &mut impl Write, response: &IpcResponse) -> Result<(), HostError> {
    let payload = serde_json::to_vec(response).map_err(|_| HostError::InvalidMessage)?;
    let length = u32::try_from(payload.len()).map_err(|_| HostError::MessageTooLarge)?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

fn launch_desktop() {
    if let Ok(executable) = std::env::var("SCROBBLE_BRIDGE_APP") {
        let _ = Command::new(executable).spawn();
        return;
    }
    #[cfg(target_os = "macos")]
    let _ = Command::new("open")
        .args(["-g", "-j", "-a", "Scrobble Bridge"])
        .spawn();
    #[cfg(windows)]
    if let Ok(path) = std::env::current_exe().and_then(|path| {
        path.parent()
            .map(|parent| parent.join("scrobble-bridge-desktop.exe"))
            .ok_or_else(|| io::Error::other("native host has no parent directory"))
    }) {
        let _ = Command::new(path).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_frame_round_trip() {
        let response = IpcResponse::success();
        let mut frame = Vec::new();
        write_chrome_message(&mut frame, &response).unwrap();
        let payload = read_chrome_message(&mut frame.as_slice()).unwrap().unwrap();
        let decoded: IpcResponse = serde_json::from_slice(&payload).unwrap();
        assert!(decoded.ok);
    }

    #[test]
    fn oversized_frame_is_rejected_before_allocation() {
        let length = u32::try_from(MAX_MESSAGE_BYTES + 1).unwrap();
        let bytes = length.to_le_bytes();
        let mut frame = bytes.as_slice();
        assert!(matches!(
            read_chrome_message(&mut frame),
            Err(HostError::MessageTooLarge)
        ));
    }
}
