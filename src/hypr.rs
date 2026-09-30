use std::{
    env,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use serde::Deserialize;

use crate::geometry::Vec2;

/// Talks to Hyprland over its control socket directly, so no `hyprctl` process is spawned per frame.
pub struct Hyprland {
    socket: PathBuf,
}

#[derive(Deserialize)]
pub struct Monitor {
    pub name: String,
    pub focused: bool,
}

#[derive(Deserialize)]
pub struct Client {
    pub address: String,
    pub title: String,
}

#[derive(Deserialize)]
struct CursorPos {
    x: f32,
    y: f32,
}

impl Hyprland {
    pub fn from_env() -> Result<Self, String> {
        let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .map_err(|_| "roose only runs on Hyprland (HYPRLAND_INSTANCE_SIGNATURE is not set)")?;
        let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        let socket = PathBuf::from(runtime_dir)
            .join("hypr")
            .join(signature)
            .join(".socket.sock");
        if !socket.exists() {
            return Err(format!(
                "Hyprland socket {} does not exist",
                socket.display()
            ));
        }
        Ok(Self { socket })
    }

    fn request(&self, command: &str) -> io::Result<String> {
        let mut stream = UnixStream::connect(&self.socket)?;
        stream.write_all(command.as_bytes())?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }

    fn request_json<T: for<'de> Deserialize<'de>>(&self, command: &str) -> Option<T> {
        serde_json::from_str(&self.request(command).ok()?).ok()
    }

    pub fn focused_monitor(&self) -> Option<String> {
        self.request_json::<Vec<Monitor>>("j/monitors")?
            .into_iter()
            .find(|m| m.focused)
            .map(|m| m.name)
    }

    pub fn cursor(&self) -> Option<Vec2> {
        self.request_json::<CursorPos>("j/cursorpos")
            .map(|c| Vec2::new(c.x, c.y))
    }

    pub fn clients(&self) -> Vec<Client> {
        self.request_json("j/clients").unwrap_or_default()
    }

    pub fn dispatch(&self, dispatchers: &[String]) {
        let batch: Vec<String> = dispatchers
            .iter()
            .map(|d| format!("dispatch {d}"))
            .collect();
        let _ = self.request(&format!("[[BATCH]]{}", batch.join(";")));
    }
}
